<p align="center">
  <h1 align="center">Pomme</h1>
  <p align="center">A high-performance Minecraft: Java Edition client, written from scratch in Rust and Vulkan</p>
  <p align="center">
    <a href="https://github.com/PommeMC/Client/actions/workflows/ci.yml"><img src="https://github.com/PommeMC/Client/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <img src="https://img.shields.io/badge/Minecraft-1.20%20to%2026.3-5D9C3F" alt="Minecraft 1.20 to 26.3">
    <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-2A2470" alt="Windows, macOS and Linux">
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="GPL-3.0 license"></a>
  </p>
  <p align="center">
    <a href="#download"><b>Download</b></a> · <a href="https://discord.gg/ucBA55bHPR"><b>Discord</b></a> · <a href="./BUILDING.md"><b>Build from source</b></a> · <a href="./CONTRIBUTING.md"><b>Contributing</b></a>
  </p>
</p>

---

Pomme contains no Mojang code, and joins vanilla servers from 1.20 through 26.3.

<p align="center">
  <img width="100%" alt="Pomme flying over a jungle" src="https://github.com/user-attachments/assets/c35e717e-3535-4f4f-9539-9e62dd482e04" />
</p>

## iPhone / iOS IPA — VERY EARLY ALPHA

> [!CAUTION]
> **This is a very, very early experimental alpha.** It is a community iOS port of PommeMC, not a stable release and not an official build supported by the upstream maintainers. Expect crashes, missing features, rendering problems, broken controls, and devices or iOS versions that may not work. Back up important data and do not rely on this build.

The project is a Rust-based Minecraft: Java Edition client being adapted for iPhone/iOS. The IPA is distributed unsigned and must be signed with your own Apple ID/developer certificate before iOS will install it. Download the newest available build from the [Releases page](https://github.com/hnhsgd6c9p-glitch/PommeMC-iOS-IPA/releases). Only use an IPA attached to a release that is marked as a successful build; GitHub Actions artifacts may be temporary.

### Install with AltStore Classic (recommended)

1. On your Mac or Windows PC, install [AltServer](https://altstore.io/) and follow the [official AltStore installation guide](https://faq.altstore.io/altstore-classic/how-to-install-altstore).
2. Connect your iPhone by USB, trust the computer, and use AltServer's menu to install AltStore on the phone. The first setup requires your Apple ID.
3. Download the Pomme unsigned `.ipa` from this repository's [Releases](https://github.com/hnhsgd6c9p-glitch/PommeMC-iOS-IPA/releases) page.
4. Open AltStore on your iPhone, go to **My Apps**, tap **+**, select the downloaded IPA in Files, and let AltStore sign and install it.
5. With a free Apple ID, sideloaded apps normally need refreshing every 7 days. Keep AltServer/AltStore configured and refresh before expiry.

Official links: [AltStore](https://altstore.io/) · [AltStore install guide](https://faq.altstore.io/altstore-classic/how-to-install-altstore).

### Install with Sideloadly

1. Download [Sideloadly](https://sideloadly.io/) for macOS or Windows.
2. Connect and trust your iPhone, then open Sideloadly.
3. Drag the Pomme `.ipa` into the IPA field.
4. Enter your Apple ID in Sideloadly, select the connected device, and start sideloading.
5. If iOS asks, trust the developer profile under **Settings → General → VPN & Device Management**.

A free Apple ID generally gives a 7-day signing period; a paid Apple Developer account can provide a longer period. Sideloadly advertises automatic re-signing, but the computer/refresh service must be available and configured. [Sideloadly download and instructions](https://sideloadly.io/).

### Run it inside LiveContainer

LiveContainer is an app launcher/container, not an emulator. It can run compatible iOS apps inside its own app environment. Pomme's Vulkan/graphics, memory, input, and platform requirements may not all work inside LiveContainer; compatibility is **not guaranteed**.

1. First install the official [LiveContainer](https://github.com/LiveContainer/LiveContainer) using its [official installation guide](https://livecontainer.github.io/docs/installation). For standalone LiveContainer, use a supported AltStore/SideStore version.
2. **Do not install LiveContainer itself with Sideloadly.** The LiveContainer maintainers explicitly say Sideloadly is unsupported for installing LiveContainer because required signing/entitlement setup may be wrong.
3. Open LiveContainer → **Settings** → **Import Certificate from AltStore/SideStore** and complete the certificate export/import prompt.
4. Run **JIT-Less Mode Diagnose** and confirm the test passes.
5. In LiveContainer, tap **+**, choose the Pomme IPA from Files, install it, then select Pomme in LiveContainer to launch it.

Official links: [LiveContainer GitHub](https://github.com/LiveContainer/LiveContainer) · [Installation guide](https://livecontainer.github.io/docs/installation) · [Standalone setup](https://livecontainer.github.io/docs/installation/standalone).

### Important limitations

- An unsigned IPA is not directly installable by iOS; a sideloading tool must sign it first.
- A successful install does not mean the app will launch or run correctly. This is an experimental port.
- LiveContainer is optional. Try a direct AltStore/Sideloadly install first if you only want to test the app itself.
- Do not enter important account credentials into unofficial repackaged builds. Download this project's IPA only from this repository's release page.
- Report reproducible bugs with your iPhone model, iOS version, install method, and crash/log details.

## Download

Get Pomme from the [latest release](https://github.com/PommeMC/Client/releases/latest):

- **Windows x64**: `pomme-client-windows-x64-setup.exe`
- **macOS (Apple Silicon)**: `pomme-client-macos-arm64.dmg`, then drag Pomme to Applications
- **Linux x64**: `pomme-client-linux-x64-gnu.AppImage`

It signs you in with your Microsoft account and keeps itself up to date. You
need to own Minecraft: Java Edition. The builds aren't signed yet, so the first
launch needs one extra step, described in the release notes.

Early Android and iOS builds build from source: see [BUILDING.md](./BUILDING.md).

## Why Pomme?

- **No JVM**: a native binary, so there's no Java to install or tune.
- **Vanilla behaviour**: physics, rendering, menus and networking are ported line
  by line from the decompiled game, and checked against it.
- **Tested against real servers**: scripted [in-game tests](./tests/ingame/README.md)
  run the client against a vanilla 26.2 server and screenshot the results.
- **Many versions, one client**: per-version protocol tables are generated from
  each version's decompiled source, and other versions are translated to 26.2.

> [!WARNING]
> Pomme is in active development. Expect missing features and bugs, and please
> report them in [Issues](https://github.com/PommeMC/Client/issues) or on Discord.

## Features

- **Rendering**: chunk meshing, GPU frustum and cave-occlusion culling, smooth lighting, water and lava, mobs with skins, particles, weather, clouds and sky
- **Singleplayer**: worlds run on [SteelMC](https://github.com/Steel-Foundation/SteelMC), a Rust server embedded in the client
- **Audio**: vanilla sound events, music and subtitles through OpenAL Soft
- **Resource packs**: local and server-sent packs, applied live
- **Inventories**: survival and creative inventories, crafting, furnaces, anvils, enchanting, chests, bundles and the recipe book
- **Controls**: rebindable keys and controllers on desktop, touch controls on phones
- **Friends**: the Minecraft friends list, requests and who's online
- **Updates**: installs per platform and keeps itself up to date

<details>
<summary>Everything else</summary>

- Vanilla-exact movement, collision, swimming and drowning
- Block entities, dropped items, block overlays and hand animation
- HUD and menus: health, hunger, air, status effects, hotbar, boss bars, titles, waypoints, advancement toasts, player list, F3 debug, chat, pause menu, options, server list, world list
- Create, edit and delete singleplayer worlds from the menu
- Microsoft sign-in from the menu, with tokens stored in the OS keyring
- Secure chat: signed messages, as vanilla sends them
- Discord presence for the menu, servers and worlds

</details>

## Building from source

See [BUILDING.md](./BUILDING.md) for building the client, running
the client standalone, and the development commands.

## Contributing

Contributions are welcome.
Please open an issue first to discuss what you'd like to change, and see
[CONTRIBUTING.md](./CONTRIBUTING.md) for the workflow.

## License

Pomme is licensed under the [GNU General Public License v3.0 or later](./LICENSE)
(GPL-3.0-or-later). Copyright (C) 2026 Purdze.

`pomme-singleplayer` links [SteelMC](./third_party/SteelMC) and is therefore
AGPL-3.0-or-later. The [allocator crate](./pomme-gpu-allocator) is MIT, a port
of the [gpu-allocator crate](https://github.com/Traverse-Research/gpu-allocator)
by Traverse Research, and [pomme-block](./pomme-block) is likewise MIT. See
[THIRD_PARTY_LICENSES.md](./THIRD_PARTY_LICENSES.md) for the other third-party code.

Pomme is not affiliated with or endorsed by Mojang Studios or Microsoft.
Minecraft is a trademark of Mojang Studios.

## Community

[![Discord](https://img.shields.io/discord/1483578136544280618?color=5865F2&label=Discord&logo=discord&logoColor=white)](https://discord.gg/ucBA55bHPR)
[![Sponsor](https://img.shields.io/badge/Sponsor-Purdze-ea4aaa?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/Purdze)

<details>
<summary>Star history</summary>

<a href="https://www.star-history.com/?repos=PommeMC%2FClient&type=date&logscale=&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=PommeMC/Client&type=date&theme=dark&logscale&legend=top-left&sealed_token=r7ib_n7xrIE4aUaRqlz9KCFjr5-Cpocxw6LPtk06-CcFfxLAEjzgPfz5bSzzGcSnbhRnkbN6KwRem4jwXnRJzcsuZYIZX5tyIAmYKFIc60c_2HBXRhGHA4rlvmyr2MXfeZDe2thqz-maf8Tdh5FxHAWJUFflGQP1JjoIfbyUZ0GW6pMxxJ8qfga5eta0" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=PommeMC/Client&type=date&logscale&legend=top-left&sealed_token=r7ib_n7xrIE4aUaRqlz9KCFjr5-Cpocxw6LPtk06-CcFfxLAEjzgPfz5bSzzGcSnbhRnkbN6KwRem4jwXnRJzcsuZYIZX5tyIAmYKFIc60c_2HBXRhGHA4rlvmyr2MXfeZDe2thqz-maf8Tdh5FxHAWJUFflGQP1JjoIfbyUZ0GW6pMxxJ8qfga5eta0" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=PommeMC/Client&type=date&logscale&legend=top-left&sealed_token=r7ib_n7xrIE4aUaRqlz9KCFjr5-Cpocxw6LPtk06-CcFfxLAEjzgPfz5bSzzGcSnbhRnkbN6KwRem4jwXnRJzcsuZYIZX5tyIAmYKFIc60c_2HBXRhGHA4rlvmyr2MXfeZDe2thqz-maf8Tdh5FxHAWJUFflGQP1JjoIfbyUZ0GW6pMxxJ8qfga5eta0" />
 </picture>
</a>

</details>
