{
  description = "mikrotik-rs — pinned toolchain via rust-overlay";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

    # One tool, one input. cargo-semver-checks reads rustdoc's unstable JSON, so it has to be
    # newer than the compiler emitting it: 26.05 ships 0.47.0, which supports formats v56 and
    # v57, and rustc 1.99 emits v61. Pulling the whole workspace onto unstable to fix one tool
    # would unpin everything else, so the release channel stays and this is scoped to the
    # package that cannot live on it. Drop the input when 26.05 catches up.
    nixpkgs-semver.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default-linux";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      perSystem = { system, ... }:
        let
          pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ (import inputs.rust-overlay) ];
          };

          # See the `nixpkgs-semver` input.
          cargo-semver-checks = inputs.nixpkgs-semver.legacyPackages.${system}.cargo-semver-checks;

          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

          # Compiled for but never run, checked by `just targets`. Must be complete:
          # this *replaces* rust-toolchain.toml's targets rather than extending them.
          # thumbv6m is Cortex-M0+ (RP2040) — no compare-and-swap, which is what
          # keeps an atomic from creeping back into the protocol core.
          crossTargets = [
            "thumbv6m-none-eabi"
            "thumbv7em-none-eabihf"
            "aarch64-apple-darwin"
            "aarch64-unknown-linux-gnu"
          ];

          # So devShells.msrv is a real compiler of that version. `.0` because
          # rust-version is a two-component floor.
          msrv = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.rust-version;

          # What the check recipes need, and so what CI needs. Split from devTools
          # because CI enters this shell once per job and the editor tooling is heavy.
          checkTools = with pkgs; [
            just
            cargo-nextest
            cargo-hack
            cargo-deny
            cargo-machete
            actionlint
            taplo
            typos
            yq-go
            git
          ] ++ [ cargo-semver-checks ];

          # Only useful to a human: watching, previewing a release.
          devTools = with pkgs; [
            bacon
            cargo-outdated
            cargo-edit
            cargo-flamegraph
            # The reason this list is split from checkTools. Only `just release-check` uses it;
            # CI gets release-plz from its action.
            release-plz
          ];
        in
        {
          devShells = {
            # A superset of `ci`, so `just all` works straight from direnv.
            default = pkgs.mkShell {
              packages = [
                # rust-src here rather than in rust-toolchain.toml so CI skips it.
                (toolchain.override {
                  targets = crossTargets;
                  extensions = [ "rust-src" ];
                })
              ]
              ++ (with pkgs; [
                rust-analyzer
                mold
              ])
              ++ checkTools
              ++ devTools;

              # Not set in `ci`: RUSTFLAGS is one string, so occupying it forces
              # every caller to append rather than assign.
              RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
            };

            # What CI enters.
            ci = pkgs.mkShell {
              packages = [ (toolchain.override { targets = crossTargets; }) ] ++ checkTools;

              # `nix develop` inherits anything it does not set, so entering this
              # from a direnv-activated `default` would drag RUSTFLAGS and mold in.
              shellHook = "unset RUSTFLAGS";
            };

            # `minimal`: checking needs neither clippy nor rustfmt.
            msrv = pkgs.mkShell {
              packages = [ pkgs.rust-bin.stable."${msrv}.0".minimal pkgs.just ];
              shellHook = "unset RUSTFLAGS";
            };
          };

          formatter = pkgs.nixpkgs-fmt;
        };
    };
}
