{
  rustPlatform,
  lib,
  makeWrapper,
  strace,
}:

rustPlatform.buildRustPackage {
  pname = "bubblepolicy-unwrapped";
  version = "0.1.0";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.lock
      ./Cargo.toml
      ./src
      ./strace-open-parser
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ makeWrapper ];

  postInstall = ''
    wrapProgram $out/bin/bubblepolicy \
      --suffix PATH : "${lib.makeBinPath [ strace ]}"
  '';
}
