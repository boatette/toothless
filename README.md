# toothless

A Night Fury that lives in the corner of your Wayland desktop and reacts to how hard your computer is working.

| CPU | What he does |
| --- | --- |
| under 10% | curled up asleep, breathing, snoring little z's |
| 10–50% | sitting up and blinking now and then |
| 50–85% | hopping and flapping, faster the busier it gets |
| over 85% | charging a plasma blast |

Click him and he hops and grins for a couple of seconds, whatever his mood. Right-click him and a speech bubble tells you the CPU usage, updating as it changes, until it pops after five seconds or you right-click again.

He is an ordinary Wayland client on wlr-layer-shell, so he runs on aion, Hyprland, sway, niri and anything else that implements it. By default he sits on the `bottom` layer: above the wallpaper, behind your windows.

The sprites are original pixel art, drawn as text grids in [`sprites/`](sprites/), one character per pixel. The palette is in `src/sprite.rs`. To look at every pose without a compositor:

```sh
cargo run --example preview -- sheet.ppm
```

## Building

```sh
nix develop   # cargo, clippy, rust-analyzer
cargo test
cargo run     # in a running Wayland session

nix build     # or build the package outright
```

## Configuration

`~/.config/toothless/config.toml`. Every key is optional; see [`config.example.toml`](config.example.toml) for all of them and their defaults.

## Home Manager

```nix
{
  inputs.toothless = {
    url = "git+file:///home/boatette/Projects/toothless";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

```nix
{ inputs, ... }:
{
  imports = [ inputs.toothless.homeModules.default ];

  programs.toothless = {
    enable = true;
    settings.corner = "bottom-right";
  };
}
```

This installs him and starts him from a systemd user service bound to `graphical-session.target`.

Toothless and *How to Train Your Dragon* belong to DreamWorks Animation. This is a personal fan project.
