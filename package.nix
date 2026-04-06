{
  rustPlatform,
  lib,
  makeWrapper,
  installShellFiles,
  pandoc,
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
      ./man
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    makeWrapper
    installShellFiles
    pandoc
  ];

  postInstall = ''
    wrapProgram $out/bin/bubblepolicy \
      --suffix PATH : "${lib.makeBinPath [ strace ]}"

    # Convert markdown man pages to roff format and install
    mkdir -p man/man1
    for md in man/md/*.md; do
      name="''${md%.md}"
      section="$(basename "$name" | grep -o '[0-9]$' || echo "1")"
      basename="$(basename "$name")"
      pandoc -s -t man -o "man/man''${section}/''${basename}.''${section}" "$md"
    done
    installManPage man/man1/*.1
  '';
}
