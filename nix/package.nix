{
  lib,
  rustPlatform,
  versionCheckHook,
}:

rustPlatform.buildRustPackage {
  pname = "once";
  version = "0.4.1";

  src = lib.cleanSource ../.;
  cargoLock.lockFile = ../Cargo.lock;

  nativeInstallCheckInputs = [ versionCheckHook ];
  doInstallCheck = true;

  meta = {
    description = "Recognize accepted Nix build-trace realizations";
    homepage = "https://github.com/closure-labs/once";
    changelog = "https://github.com/closure-labs/once/blob/v0.4.1/CHANGELOG.md";
    license = lib.licenses.asl20;
    mainProgram = "once";
    platforms = lib.platforms.linux;
  };
}
