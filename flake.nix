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

          # Platforms compiled for but never run, checked by `just targets`. This list must be
          # complete: the override *replaces* rust-toolchain.toml's `targets` rather than
          # extending it.
          #
          # thumbv7em-none-eabihf belongs here and is deliberately absent. mikrotik-embassy is
          # unconditionally #![no_std] and mikrotik-proto is no_std by default, but neither can
          # currently be built for a target without std: Tag wraps uuid::Uuid, uuid's `v4`
          # feature pulls getrandom, and getrandom compile_error!s on bare metal. A host build
          # with --no-default-features still has std in the sysroot, which is why that has never
          # been caught. Add this target back in the commit that gives Tag a no_std source —
          # until then it would only ever be a red job.
          crossTargets = [
            "aarch64-apple-darwin"
            "aarch64-unknown-linux-gnu"
          ];

          # Derived so `devShells.msrv` is an actual compiler of that version, rather than an env
          # var pointed at a different one. `.0` because rust-version is a two-component floor
          # and rust-overlay keys on full releases. Keep clippy.toml's `msrv` in step.
          msrv = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.rust-version;

          # What the check recipes need, and so what CI needs. Separate from devTools because CI
          # enters this shell once per job and the editor tooling is where the weight is.
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
          ]
          # In `just all` and in CI, unlike the upstream template: this workspace has four
          # crates already on the registry, so the baseline exists from day one. release-plz
          # also shells out to it to decide version bumps.
          ++ [ cargo-semver-checks ];

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
                # rust-src lets rust-analyzer resolve `std`. An extension here rather than in
                # rust-toolchain.toml so CI does not download it.
                (toolchain.override {
                  targets = crossTargets;
                  extensions = [ "rust-src" ];
                })
              ]
              ++ (with pkgs; [
                # Pinned to this toolchain, so it beats any rust-analyzer further down PATH.
                rust-analyzer
                mold
              ])
              ++ checkTools
              ++ devTools;

              # Not set in `ci`: RUSTFLAGS is one string, so occupying it forces every caller to
              # append rather than assign.
              RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
            };

            # What CI enters.
            ci = pkgs.mkShell {
              packages = [ (toolchain.override { targets = crossTargets; }) ] ++ checkTools;

              # `nix develop` inherits any variable it does not set, so entering this from a
              # direnv-activated `default` drags RUSTFLAGS in. mold leaks via PATH too, so
              # without this the failure only shows on a machine with the flag but not the linker.
              shellHook = "unset RUSTFLAGS";
            };

            # One recipe's worth. `minimal` suffices: checking needs neither clippy nor rustfmt.
            msrv = pkgs.mkShell {
              packages = [ pkgs.rust-bin.stable."${msrv}.0".minimal pkgs.just ];
              shellHook = "unset RUSTFLAGS";
            };
          };

          formatter = pkgs.nixpkgs-fmt;
        };
    };
}
