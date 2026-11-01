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
      ./benches
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
  ];

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;

  # Keep local cargo builds to ~90% of the machine's cores.
  enterShell = ''
    export CARGO_BUILD_JOBS=$(( $(nproc) * 9 / 10 > 0 ? $(nproc) * 9 / 10 : 1 ))
  '';

  outputs = { inherit conformal-visualizer; };

  scripts.build-remote.exec = ''
    # -j 0: no local build slots, so every derivation goes to nixbuild.net
    devenv build -j 0 outputs.conformal-visualizer "$@"
  '';

  enterTest = ''
    cargo test
  '';
}
