{ lib
, rustPlatform
, makeWrapper
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
  version = "0.2.1";
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
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibraries} \
      --run 'if [ ! -x /run/current-system/sw/bin/nvidia-gpu-manager-nvidia-smi ]; then printf "%s\\n" "nvidia-gpu-manager requires the NixOS module-managed nvidia-smi compatibility wrapper. Enable programs.nvidia-gpu-manager and rebuild the system." >&2; exit 78; fi'

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
