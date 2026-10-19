#!/usr/bin/env python3
"""Fetch Once bundles from successful push runs of a protected producer workflow."""
import argparse
import io
import json
import os
from pathlib import Path, PurePosixPath
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request
import zipfile

MAX_ARCHIVE = 64 * 1024 * 1024


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class GitHub:
    def __init__(self, repository, token, api_url):
        if not token:
            raise ValueError("github-token is required for artifact lookup")
        self.repository = repository
        self.base = api_url.rstrip("/") + "/repos/" + repository
        self.token = token

    def request(self, path):
        request = urllib.request.Request(self.base + path, headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        })
        try:
            with urllib.request.build_opener(NoRedirect).open(request, timeout=60) as response:
                return response.read(MAX_ARCHIVE + 1)
        except urllib.error.HTTPError as error:
            if error.code in (404, 410):
                return None
            if error.code == 302 and path.endswith("/zip"):
                location = error.headers["Location"]
                if urllib.parse.urlsplit(location).scheme != "https":
                    raise ValueError("artifact download redirect must use HTTPS") from error
                # Object storage never receives the repository bearer token.
                with urllib.request.urlopen(location, timeout=60) as response:
                    return response.read(MAX_ARCHIVE + 1)
            raise

    def json(self, path):
        payload = self.request(path)
        return None if payload is None else json.loads(payload)


def pages(api, path, field):
    page = 1
    separator = "&" if "?" in path else "?"
    while True:
        payload = api.json(path + separator + urllib.parse.urlencode({"per_page": 100, "page": page}))
        if payload is None:
            return
        values = payload[field]
        yield from values
        if len(values) < 100:
            return
        page += 1


def find_artifacts(api, workflow, branch, name):
    def quote(value):
        return urllib.parse.quote(str(value), safe="")
    protection = api.json("/branches/" + quote(branch))
    if protection is None or protection.get("name") != branch or protection.get("protected") is not True:
        raise ValueError("producer-branch must identify an existing protected branch")
    definition = api.json("/actions/workflows/" + quote(workflow))
    if definition is None:
        return
    workflow_id = definition["id"]
    # Filter by derivation identity on the server. Scanning every producer run
    # makes a new derivation's miss proportional to repository history.
    query = urllib.parse.urlencode({"name": name})
    runs = {}
    for artifact in pages(api, f"/actions/artifacts?{query}", "artifacts"):
        if artifact.get("name") != name or artifact.get("expired") is not False:
            continue
        attached_run = artifact.get("workflow_run") or {}
        run_id = attached_run.get("id")
        if not isinstance(run_id, int) or isinstance(run_id, bool) or run_id <= 0:
            continue
        if run_id not in runs:
            runs[run_id] = api.json(f"/actions/runs/{run_id}")
        run = runs[run_id]
        if not (run is not None
                and run.get("id") == run_id
                and run.get("workflow_id") == workflow_id
                and run.get("head_branch") == branch
                and run.get("event") == "push"
                and run.get("status") == "completed"
                and run.get("conclusion") == "success"
                and (run.get("head_repository") or {}).get("full_name") == api.repository):
            continue
        if artifact.get("size_in_bytes", MAX_ARCHIVE + 1) > MAX_ARCHIVE:
            raise ValueError("Once bundle artifact exceeds size limit")
        yield int(artifact["id"])


def extract_bundle(payload, destination):
    if len(payload) > MAX_ARCHIVE:
        raise ValueError("Once bundle archive exceeds size limit")
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        members = archive.infolist()
        if not members or len(members) > 10000 or sum(m.file_size for m in members) > MAX_ARCHIVE:
            raise ValueError("Once bundle has invalid expanded size")
        seen = set()
        for member in members:
            path = PurePosixPath(member.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in member.filename
                    or not path.parts or path in seen
                    or stat.S_ISLNK(member.external_attr >> 16)):
                raise ValueError("unsafe path in Once bundle archive")
            seen.add(path)
        destination.mkdir(parents=True, exist_ok=False)
        archive.extractall(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("workflow", "branch", "name", "output"):
        parser.add_argument("--" + option, required=True)
    arguments = parser.parse_args()
    try:
        repository = os.environ["GITHUB_REPOSITORY"]
        if len(repository.split("/")) != 2 or any(p in ("", ".", "..") for p in repository.split("/")):
            raise ValueError("invalid GITHUB_REPOSITORY")
        api = GitHub(repository, os.environ.get("ONCE_ACTION_GITHUB_TOKEN", ""),
                     os.environ.get("GITHUB_API_URL", "https://api.github.com"))
        for artifact_id in find_artifacts(api, arguments.workflow, arguments.branch, arguments.name):
            payload = api.request(f"/actions/artifacts/{artifact_id}/zip")
            if payload is None:
                continue
            extract_bundle(payload, Path(arguments.output))
            return 0
        return 10
    except (ValueError, KeyError, OSError, urllib.error.URLError, zipfile.BadZipFile) as error:
        print(f"Once artifact transport failed: {error}", file=sys.stderr)
        return 40


if __name__ == "__main__":
    sys.exit(main())
