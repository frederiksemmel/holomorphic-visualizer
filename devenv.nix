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
      # Fonts the app embeds.
      ./assets
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

  # Where `devenv up` serves the fast web build (see processes below).
  webDevPort = 18091;
  # Funnel only allows 443, 8443 and 10000; the first two are often in use here.
  webDevFunnelPort = 10000;
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
    watchexec
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

  # `devenv up`: rebuild the browser version on every change and serve it over
  # HTTPS (WebGPU needs a secure context). The last good build stays up while a
  # new one compiles or fails; reload the page once web-build has finished.
  processes.web-build.exec = ''
    cd "$DEVENV_ROOT"
    WEB_PROFILE=web-dev exec watchexec --on-busy-update queue \
      --watch src --watch assets --watch vendor \
      --watch web/index.html --watch web/build.sh \
      --watch Cargo.toml --watch Cargo.lock \
      -- web/build.sh
  '';
  processes.web-serve.exec = ''
    cd "$DEVENV_ROOT"
    mkdir -p web/dist
    exec miniserve -i 127.0.0.1 -p ${toString webDevPort} --index index.html \
      --compress-response --header "Cache-Control: no-store" web/dist
  '';
  # Stops with `devenv up`, unlike `tailscale funnel --bg`.
  processes.web-funnel.exec = ''
    exec tailscale funnel --https=${toString webDevFunnelPort} http://127.0.0.1:${toString webDevPort}
  '';

  scripts.build-remote.exec = ''
    # -j 0: no local build slots, so every derivation goes to nixbuild.net
    devenv build -j 0 outputs.conformal-visualizer "$@"
  '';

  enterTest = ''
    cargo test
  '';
}
