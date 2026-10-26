self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.toothless;
  tomlFormat = pkgs.formats.toml { };
in
{
  options.programs.toothless = {
    enable = lib.mkEnableOption "toothless, a Night Fury that lives in the corner of your Wayland desktop";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.toothless;
      defaultText = lib.literalExpression "toothless.packages.\${system}.toothless";
      description = "The toothless package to use";
    };

    systemd.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to run toothless from a systemd user service bound to graphical-session.target";
    };

    settings = lib.mkOption {
      type = tomlFormat.type;
      default = { };
      example = lib.literalExpression ''
        {
          corner = "bottom-left";
          scale = 3;
          excited = 40;
        }
      '';
      description = "Settings written to $XDG_CONFIG_HOME/toothless/config.toml, see config.example.toml";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."toothless/config.toml" = lib.mkIf (cfg.settings != { }) {
      source = tomlFormat.generate "toothless-config.toml" cfg.settings;
    };

    systemd.user.services.toothless = lib.mkIf cfg.systemd.enable {
      Unit = {
        Description = "A Night Fury that lives in the corner of your Wayland desktop";
        PartOf = [ "graphical-session.target" ];
        After = [ "graphical-session.target" ];
        ConditionEnvironment = "WAYLAND_DISPLAY";
      };
      Service = {
        ExecStart = lib.getExe cfg.package;
        Restart = "always";
        RestartSec = 1;
      };
      Install.WantedBy = [ "graphical-session.target" ];
    };
  };
}
