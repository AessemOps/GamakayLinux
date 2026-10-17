# Packaging

Four formats, one file set. Everything installs the same two binaries, the udev
rule, the desktop entry and its icons; what differs is the metadata each format
needs, so each format uses the tool that owns it rather than a hand-written
`control` or `.spec`.

| format | built by | artefact | tool |
|---|---|---|---|
| Arch / CachyOS | `make package-arch` | `gamakaylinux-<ver>-1-<arch>.pkg.tar.zst` | `makepkg`, `packaging/arch/PKGBUILD` |
| Debian / Ubuntu | `make package-deb` | `gamakaylinux_<ver>_amd64.deb` | `nfpm` |
| Fedora / RHEL | `make package-rpm` | `gamakaylinux-<ver>-1.x86_64.rpm` | `nfpm` |
| any Linux (portable) | `make package-appimage` | `GamakayLinux-<ver>-x86_64.AppImage` | `appimagetool` |

`make packages` builds the deb, the rpm and the AppImage; the Arch package needs
an Arch host (or container) for `makepkg`, and its `PKGBUILD` sources the
published tag tarball — so `make package-arch` is a local reproduction of what a
user builds from the release, while CI synthesises the same tarball from the
working tree so it can run before a tag exists.

## What a package installs

```
/usr/bin/gamakay                       CLI
/usr/bin/gamakay-gui                   iced GUI
/usr/lib/udev/rules.d/70-gamakay.rules grants the local user the hidraw node
/usr/share/applications/gamakay.desktop
/usr/share/icons/hicolor/<size>/apps/gamakay.png  16..512
LICENSE, CREDITS.md, README.md         per-format location
```

The udev rule is what lets a normal user configure the keyboard: without it the
hidraw node is root-only and every write fails with `Permission denied`. The
package installs it; the postinstall (deb/rpm) and `post_install` (Arch) then
`udevadm control --reload-rules` plus a hidraw trigger so it takes effect without
a reboot. An AppImage cannot do that — see below.

## Building locally

`nfpm` and `appimagetool` are not in every base system. Both ship as static
binaries; fetch them once and point the Makefile at them:

```bash
mkdir -p ~/.local/bin
curl -fsSL https://github.com/goreleaser/nfpm/releases/download/v2.48.0/nfpm_2.48.0_Linux_x86_64.tar.gz \
  | tar -xz -C ~/.local/bin nfpm
curl -fsSL -o ~/.local/bin/appimagetool \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x ~/.local/bin/appimagetool

make release                     # the packages ship target/release
make package-deb package-rpm APPIMAGETOOL=$HOME/.local/bin/appimagetool
make package-verify              # what actually landed in them
```

`make package-verify` reads the artefacts rather than the config: `ar`/`tar` for
the `.deb` (no `dpkg-deb` needed) and `rpm -qpi/-qpl` with a scratch database for
the `.rpm`.

The icons are committed, so a package build needs no image tooling. They are
generated from `packaging/icons/gamakay.svg` by `scripts/make-icons.sh` when the
SVG changes.

## Versions

The workspace `Cargo.toml` version is the single source. `nfpm` and the AppImage
take it from `VERSION` in the environment (the Makefile derives it), and the one
place it is written by hand besides Cargo — the PKGBUILD's `pkgver`, which Arch
requires as a literal — is checked by `scripts/check-release-version.sh`, which
also refuses a tag that disagrees and a crate that stops inheriting the workspace
version. The release workflow runs it as its first job.

## The release workflow

`.github/workflows/release.yml` runs on a `v*` tag (and by hand, which builds and
verifies without publishing). It builds on **ubuntu-22.04** deliberately: the
artefacts would otherwise inherit a newer glibc, and a deb built on `ubuntu-latest`
would refuse to install on 22.04. The Arch package is built in an `archlinux`
container, as `makepkg` requires, with an unprivileged build user since it
refuses to run as root.

Each format is then **installed into a container of its own distribution and
run** — `gamakay --version`, which needs no keyboard — so a package that cannot
start fails in CI rather than on a user's machine. That is the check Phase 6
exists to make.

## Decisions worth knowing

**The AppImage bundles the binaries, the desktop entry and the icon. It does not
bundle the display stack.** `gamakay-gui` loads `libxkbcommon`, Wayland/X11 and
the GL/Vulkan loader with `dlopen` at runtime, so none of them appear in `ldd`;
bundling them fixes the versions at build time and would clash with a newer host
just as often as it helps. Every desktop already provides them.

**It does not bundle a font either.** The UI's legends are drawn with the system
sans — `DejaVu Sans` on the systems these packages target — and the packages
depend on the right font package per distribution (`fonts-dejavu-core`,
`dejavu-sans-fonts`, `ttf-dejavu`). Only the emoji used for a few keys is
compiled into the binary.

**The AppImage ships the rule but cannot activate it.** Use it read-only, or
install the rule by hand (`/usr/share/udev/70-gamakay.rules` inside the
AppImage, documented in the README).

**One AppImage, two entry points.** `AppRun` sends a CLI subcommand to `gamakay`
and everything else to `gamakay-gui`, so the archive is both the desktop app and
a usable CLI — and so CI can run it with no display. `info` is ambiguous (a CLI
subcommand and a GUI tab); the CLI wins, and `--gui info` opens the tab.
