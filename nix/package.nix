{ lib
, rustPlatform
, makeWrapper
, polkit
, wayland
, libxkbcommon
, libGL
, libX11
, libXcursor
, libXi
, libXrandr
}:

let
  runtimeLibraries = [
    wayland
    libxkbcommon
    libGL
    libX11
    libXcursor
    libXi
    libXrandr
  ];
in
rustPlatform.buildRustPackage {
  pname = "nvidia-gpu-manager";
  version = "0.1.0";
  src = lib.cleanSourceWith {
    src = ../.;
    filter = path: type:
      let name = baseNameOf (toString path);
      in name != "target" && name != "result";
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [ makeWrapper ];
  buildInputs = runtimeLibraries;

  postInstall = ''
    wrapProgram $out/bin/nvidia-gpu-manager \
      --prefix PATH : ${lib.makeBinPath [ polkit ]} \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibraries}

    install -Dm644 ${../nvidia-gpu-manager.desktop} \
      $out/share/applications/nvidia-gpu-manager.desktop
  '';

  meta = {
    description = "NVIDIA GPU monitoring and control GUI for NixOS";
    license = lib.licenses.mit;
    mainProgram = "nvidia-gpu-manager";
    platforms = lib.platforms.linux;
  };
}
