{ pkgs, lib, config, ... }:

let
  # Bevy/wgpu dlopen() these at runtime (Vulkan/GL, Wayland, X11, keyboard, fonts).
  runtimeLibs = with pkgs; [
    libGL
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
    vulkan-loader
  ];

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./vendor
    ];
  };

  unwrapped = config.languages.rust.import src {
    crateOverrides = pkgs.defaultCrateOverrides // {
      # Bevy's text stack (fontique) links fontconfig; nixpkgs has no override for it yet.
      yeslogic-fontconfig-sys = _: {
        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ pkgs.fontconfig ];
      };
      # Their build scripts unwrap CARGO_ENCODED_RUSTFLAGS, which only cargo sets.
      av-scenechange = _: { CARGO_ENCODED_RUSTFLAGS = ""; };
      rav1e = _: { CARGO_ENCODED_RUSTFLAGS = ""; };
    };
  };

  # Must match the wasm-bindgen crate version in Cargo.lock.
  wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
    src = pkgs.fetchCrate {
      pname = "wasm-bindgen-cli";
      version = "0.2.130";
      hash = "sha256-9Og1O/6/VC3sc5hBjoIWBErmLW2UHGMsTQO+6+m/dng=";
    };
    cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
      inherit src;
      inherit (src) pname version;
      hash = "sha256-j81sUf1m5yXU4FJfGj7GV+hdqVCV8mRmNb0QIsMCjqw=";
    };
  };

  conformal-visualizer = pkgs.symlinkJoin {
    name = "conformal-visualizer-${unwrapped.version}";
    paths = [ unwrapped ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      wrapProgram $out/bin/conformal-visualizer \
        --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
    '';
    meta.mainProgram = "conformal-visualizer";
  };
in
{
  languages.rust.enable = true;

  packages = with pkgs; [
    cargo-edit
    pkg-config
    fontconfig.dev
    # Browser build: web/build.sh (lld links wasm32)
    lld
    wasm-bindgen-cli
    binaryen
    miniserve
  ];

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;

  # Keep local cargo builds to ~90% of the machine's cores.
  enterShell = ''
    export CARGO_BUILD_JOBS=$(( $(nproc) * 9 / 10 > 0 ? $(nproc) * 9 / 10 : 1 ))
  '';

  outputs = { inherit conformal-visualizer; };

  scripts.build-web.exec = ''
    "$DEVENV_ROOT/web/build.sh"
  '';
  scripts.serve-web.exec = ''
    build-web && miniserve --index index.html "$DEVENV_ROOT/web/dist"
  '';

  scripts.build-remote.exec = ''
    # -j 0: no local build slots, so every derivation goes to nixbuild.net
    devenv build -j 0 outputs.conformal-visualizer "$@"
  '';

  enterTest = ''
    cargo test
  '';
}
