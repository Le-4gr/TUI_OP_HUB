{
  description = "TUI-OP-HUB — terminal operations hub (Rust + ratatui + axum)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f:
        builtins.listToAttrs
          (map (s: { name = s; value = f s; }) systems);
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "tui-op-hub";
            version = "0.2.0";

            src = self;
            # the crate lives in a subdirectory of the repo
            sourceRoot = "source/TUI-OP-HUB";
            cargoLock.lockFile = self + "/TUI-OP-HUB/Cargo.lock";
            # tests need git + HOME on PATH — run locally / in CI instead
            # (lib tests 221/221, BDD 48/48 in debug)
            doCheck = false;

            meta = with pkgs.lib; {
              description = "Terminal operations hub: commands, scripts, workflows, secrets, configs";
              homepage = "https://github.com/Le-4gr/TUI_OP_HUB";
              license = with licenses; [ mit asl20 ];
              mainProgram = "tui-op-hub";
            };
          };
        });
    };
}
