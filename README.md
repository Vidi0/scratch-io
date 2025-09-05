# 🎮 scratch-io

**scratch-io** is a command-line tool for managing, downloading, and launching games from [itch.io](https://itch.io).

## ✨ Features

- 🔑 Authentication with the itch.io API
- 📥 Game download with automatic extraction (zip, tar.gz, tar.xz, ...)
- 🗃️ Management of installed games (list, move, delete, import)
- 🚀 Game launch with smart heuristics to find the correct executable
- 🗂️ Support for collections and game keys
- 🖼️ Automatic cover art download

## ⚡ Build

Requires [Rust](https://www.rust-lang.org/tools/install) and `cargo`:

```sh
git clone https://github.com/Vidi0/scratch-io.git
cd scratch-io
cargo build --release
```

The binary will be placed in `target/release/scratch-io`.

## 🚀 Usage

Authenticate with your itch.io API key:

```sh
scratch-io auth YOUR_API_KEY
```

Alternatively, log in using your username and password:

```sh
scratch-io login USERNAME PASSWORD
```

Download a game by its upload ID:

```sh
scratch-io download 123456
```

List installed games:

```sh
scratch-io installed
```

Launch an installed game:

```sh
scratch-io launch 123456 GAME_PLATFORM
```

See all options with:

```sh
scratch-io help
```

> [!WARNING]
> Due to how the itch.io API works, it is not possible to update a game in-place.  
> To update a game, you must remove it and install it again.

> [!NOTE]
> Launching games is determined by heuristics, so it may not always work for every game.  
> If the executable for a game is not detected correctly, please leave an issue in the repository describing your case.

## 🛠️ Environment variables

- `SCRATCH_API_KEY`: itch.io API key
- `SCRATCH_CONFIG_FILE`: Custom path for the configuration file

## 📚 References

- [itchapi.ryhn.link](https://itchapi.ryhn.link) – Unofficial itch.io API documentation
- [itch-downloader](https://github.com/BraedonWooding/itch-downloader) – Example itch.io API usage
- [itch.io docs: compatibility policy](https://docs.itch.ovh/itch/master/integrating/compatibility-policy.html)
- [itch.io docs: manifest](https://docs.itch.ovh/itch/master/integrating/manifest.html)

## 📝 Roadmap

- **Integration with Heroic Games Launcher:**  
  This project was designed with the intention of being integrated into [Heroic Games Launcher](https://heroicgameslauncher.com/).  
  **Note:** Integration is not currently implemented.

## 📝 TODO

1. Reading the game executable from the [itch.io manifest](https://docs.itch.ovh/itch/master/integrating/manifest.html)
2. Reporting game playtime to the itch.io servers
3. Ability to update and verify games packed with [butler](https://itch.io/docs/butler/)

## 📝 License

**scratch-io** is released under the GPL-3.0-or-later license.  
This project uses many third-party crates; their licenses are listed in `LICENSE-THIRD-PARTY.html`.
