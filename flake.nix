{
  description = "YuioLink - wieldy ephemeral links: every link expires";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Pin a version with rust-bin.stable."1.85.0".default if needed.
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # cargo-about 0.9.2 fixed upstream issue #309: 0.9.0 kept one license
        # file per license, chosen in directory-listing order, so the crate
        # license file differed between macOS and Linux and could credit a
        # crate with a bundled file's text. nixpkgs still ships 0.9.0 (checked
        # 2026-11-27); drop this override once it ships 0.9.2 or later.
        cargo-about = pkgs.cargo-about.overrideAttrs (finalAttrs: _old: {
          version = "0.9.3";
          src = pkgs.fetchFromGitHub {
            owner = "EmbarkStudios";
            repo = "cargo-about";
            tag = finalAttrs.version;
            hash = "sha256-Jlr4TYNZkzYo5V77n0YYrx1e8m17ELN/1+ZcR/Bl0K4=";
          };
          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            inherit (finalAttrs) pname version src;
            hash = "sha256-kNGfQQumgvtDPhiQcqc9K2ohhEsTzuySeisHcNcmig8=";
          };
        });

        # Keep transcripts / scratch dirs out of the build source so editing
        # them does not bust the Nix build cache.
        src = pkgs.lib.cleanSourceWith {
          src = pkgs.lib.cleanSource ./.;
          filter = path: _type:
            let base = baseNameOf (toString path);
            in !(pkgs.lib.hasPrefix ".claude" base)
            && base != ".tmp"
            && base != ".direnv";
        };
      in
      {
        # `nix build` -> the YuioLink binaries (server/cli, as crates land).
        packages.default = rustPlatform.buildRustPackage {
          pname = "yuiolink";
          # Single source of truth: the workspace version in Cargo.toml.
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          inherit src;
          cargoLock.lockFile = ./Cargo.lock;

          # SQLite (libsqlite3-sys) for the forthcoming server crate.
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.sqlite ];
        };

        # `nix develop` -> dev shell with the Rust toolchain + native deps.
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.sqlite
            # Third-party license texts (tools/crate-licenses) and the
            # dependency policy in deny.toml; CI runs both from this shell.
            cargo-about
            pkgs.cargo-deny
          ];
        };
      }
    );
}
