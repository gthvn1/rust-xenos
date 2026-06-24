{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Rust toolchain manager
    rustup

    # Dev tools not handled by rustup
    bacon

    # Fast linker (big win)
    lld
  ];

  # Ensures rust-analyzer works properly
  RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";

  # Fix rustup home so it doesn't fight with nix shells
  shellHook = ''
    export RUSTUP_HOME=$HOME/.rustup
    export CARGO_HOME=$HOME/.cargo
    export PATH=$CARGO_HOME/bin:$PATH

    echo ""
    echo "🦀 Rust dev shell (Nix + rustup)"
    echo "--------------------------------"
    echo "• rustup is provided by Nix"
    echo "• bacon is provided by Nix"
    echo ""
    echo "⚠️  You still need to install Rust tools via rustup:"
    echo "   rustup default stable"
    echo "   rustup component add rustfmt clippy rust-analyzer"
    echo ""
  '';
}
