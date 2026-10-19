#!/usr/bin/env python3
"""Artifact transport tests: metadata is an availability filter, signatures authorize reuse."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import urllib.error
import zipfile

spec = importlib.util.spec_from_file_location("artifacts", Path(__file__).parents[1] / "scripts/github-artifacts.py")
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


class FakeAPI:
    repository = "owner/repo"

    def __init__(self):
        self.run = dict(id=1, workflow_id=2, head_branch="main", event="push", status="completed",
                        conclusion="success", head_repository={"full_name": self.repository})
        self.artifact = dict(id=3, name="once-v1-abc", expired=False, size_in_bytes=100,
                             workflow_run={"id": 1})
        self.artifacts = None
        self.protected = True
        self.missing_workflow = False
        self.error = None
        self.calls = []

    def json(self, path):
        self.calls.append(path)
        if self.error:
            raise self.error
        if path.startswith("/branches/"):
            return dict(name="main", protected=self.protected)
        if path.startswith("/actions/artifacts?"):
            return {"artifacts": [self.artifact] if self.artifacts is None else self.artifacts}
        if path.startswith("/actions/runs/"):
            return self.run if path == "/actions/runs/1" else None
        if path.startswith("/actions/workflows/"):
            return None if self.missing_workflow else {"id": 2}
        raise AssertionError("unexpected API request: " + path)


class TransportTests(unittest.TestCase):
    def lookup(self, api):
        return list(artifacts.find_artifacts(api, "ci.yml", "main", "once-v1-abc"))

    def test_matching_successful_run(self):
        api = FakeAPI()
        self.assertEqual(self.lookup(api), [3])
        self.assertEqual(api.calls, ["/branches/main", "/actions/workflows/ci.yml",
                                     "/actions/artifacts?name=once-v1-abc&per_page=100&page=1",
                                     "/actions/runs/1"])

    def test_metadata_filters(self):
        for field, value in [("id", 999), ("workflow_id", 4), ("head_branch", "other"), ("event", "pull_request"),
                             ("status", "in_progress"), ("conclusion", "failure"),
                             ("head_repository", {"full_name": "fork/repo"})]:
            with self.subTest(field=field):
                api = FakeAPI()
                api.run[field] = value
                self.assertEqual(self.lookup(api), [])
        for field, value in [("name", "other"), ("expired", True), ("workflow_run", {"id": 999}),
                             ("workflow_run", None), ("workflow_run", {})]:
            with self.subTest(field=field):
                api = FakeAPI()
                api.artifact[field] = value
                self.assertEqual(self.lookup(api), [])

    def test_missing_workflow_is_miss(self):
        api = FakeAPI()
        api.missing_workflow = True
        self.assertEqual(self.lookup(api), [])

    def test_unprotected_branch_is_error(self):
        api = FakeAPI()
        api.protected = False
        with self.assertRaises(ValueError):
            self.lookup(api)

    def test_auth_and_network_errors_propagate(self):
        for error in [urllib.error.HTTPError("https://api.github.com", 403, "forbidden", {}, io.BytesIO()),
                      urllib.error.URLError("offline")]:
            api = FakeAPI()
            api.error = error
            try:
                with self.assertRaises(type(error)):
                    self.lookup(api)
            finally:
                if isinstance(error, urllib.error.HTTPError):
                    error.close()

    def test_extract_and_reject_unsafe_members(self):
        for name in ["bundle.json", "../outside", "/absolute", "a\\b"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                data = io.BytesIO()
                with zipfile.ZipFile(data, "w") as archive:
                    archive.writestr(name, '{"test":true}')
                destination = Path(directory) / "bundle"
                if name == "bundle.json":
                    artifacts.extract_bundle(data.getvalue(), destination)
                    self.assertTrue((destination / name).is_file())
                else:
                    with self.assertRaises(ValueError):
                        artifacts.extract_bundle(data.getvalue(), destination)
                    self.assertFalse(destination.exists())

    def test_new_derivation_miss_uses_constant_requests(self):
        api = FakeAPI()
        api.artifacts = []
        self.assertEqual(self.lookup(api), [])
        self.assertEqual(api.calls, ["/branches/main", "/actions/workflows/ci.yml",
                                     "/actions/artifacts?name=once-v1-abc&per_page=100&page=1"])

    def test_only_matching_unexpired_artifacts_fetch_runs(self):
        api = FakeAPI()
        api.artifacts = [dict(api.artifact, name="different"), dict(api.artifact, expired=True)]
        self.assertEqual(self.lookup(api), [])
        self.assertEqual(len(api.calls), 3)

    def test_pagination_searches_older_matching_artifacts(self):
        api = FakeAPI()
        original = api.json
        def paginate(path):
            if path.startswith("/actions/artifacts?"):
                api.calls.append(path)
                query = artifacts.urllib.parse.parse_qs(artifacts.urllib.parse.urlsplit(path).query)
                self.assertEqual(query["name"], ["once-v1-abc"])
                if query["page"] == ["1"]:
                    return {"artifacts": [dict(api.artifact, expired=True)] * 100}
                return {"artifacts": [api.artifact]}
            return original(path)
        api.json = paginate
        self.assertEqual(self.lookup(api), [3])
        self.assertEqual(len(api.calls), 5)
        self.assertEqual(api.calls[-1], "/actions/runs/1")

    def test_run_metadata_is_fetched_once_per_candidate_run(self):
        api = FakeAPI()
        api.artifacts = [api.artifact, dict(api.artifact, id=4)]
        self.assertEqual(self.lookup(api), [3, 4])
        self.assertEqual(api.calls.count("/actions/runs/1"), 1)

    def test_candidate_run_auth_error_propagates(self):
        api = FakeAPI()
        original = api.json
        def unauthorized_run(path):
            if path.startswith("/actions/runs/"):
                raise urllib.error.URLError("network interrupted")
            return original(path)
        api.json = unauthorized_run
        with self.assertRaises(urllib.error.URLError):
            self.lookup(api)

    def test_missing_and_expired_downloads_are_misses(self):
        for download in (None, b"unused"):
            with self.subTest(download=download), tempfile.TemporaryDirectory() as directory:
                api = FakeAPI()
                api.request = lambda path: download
                if download is not None:
                    api.artifact["expired"] = True
                args = ["github-artifacts.py", "--workflow", "ci.yml", "--branch", "main",
                        "--name", "once-v1-abc", "--output", str(Path(directory) / "bundle")]
                with mock.patch.object(artifacts, "GitHub", return_value=api), \
                     mock.patch.dict(artifacts.os.environ, {"GITHUB_REPOSITORY": api.repository}), \
                     mock.patch.object(artifacts.sys, "argv", args):
                    self.assertEqual(artifacts.main(), 10)

    def test_storage_redirect_does_not_forward_token(self):
        api = artifacts.GitHub("owner/repo", "SECRET", "https://api.github.com")
        redirect = urllib.error.HTTPError("https://api.github.com", 302, "found",
                                         {"Location": "https://storage.example/archive"}, io.BytesIO())
        response = mock.MagicMock()
        response.__enter__.return_value.read.return_value = b"zip"
        try:
            with mock.patch.object(artifacts.urllib.request, "build_opener") as opener, \
                 mock.patch.object(artifacts.urllib.request, "urlopen", return_value=response) as download:
                opener.return_value.open.side_effect = redirect
                self.assertEqual(api.request("/actions/artifacts/3/zip"), b"zip")
                download.assert_called_once_with("https://storage.example/archive", timeout=60)
        finally:
            redirect.close()

    def test_reject_symlink(self):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            info = zipfile.ZipInfo("link")
            info.external_attr = 0o120777 << 16
            archive.writestr(info, "../outside")
        with tempfile.TemporaryDirectory() as directory, self.assertRaises(ValueError):
            artifacts.extract_bundle(data.getvalue(), Path(directory) / "bundle")


if __name__ == "__main__":
    unittest.main()
