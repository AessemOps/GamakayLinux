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
/usr/share/metainfo/io.github.aessemops.gamakaylinux.metainfo.xml           AppStream, for software centres
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
  https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
echo "ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  $HOME/.local/bin/appimagetool" | sha256sum -c -
chmod +x ~/.local/bin/appimagetool

# appimagetool otherwise downloads the runtime it embeds from a rolling
# `continuous` URL with no integrity check. Pin the same one CI does.
curl -fsSL -o /tmp/runtime-x86_64 \
  https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
echo "2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d  /tmp/runtime-x86_64" | sha256sum -c -

make release                     # the packages ship target/release
make package-deb package-rpm
make package-appimage \
  APPIMAGE_RUNTIME=/tmp/runtime-x86_64 \
  APPIMAGETOOL="$HOME/.local/bin/appimagetool"
make package-verify              # what actually landed in them (needs rpm and rg)
```

`appimagetool` is itself an AppImage; on a host without FUSE run it as
`"$HOME/.local/bin/appimagetool --appimage-extract-and-run"`.

`make package-verify` reads the artefacts rather than the config: `ar`/`tar` for
the `.deb` (no `dpkg-deb` needed) and `rpm -qpi/-qpl` with a scratch database for
the `.rpm`.

The icons are committed, so a package build needs no image tooling. They are
generated from `packaging/icons/gamakay.svg` by `scripts/make-icons.sh` when the
SVG changes.

## Source integrity in the Arch package

A `PKGBUILD` cannot carry the hash of a tarball that the tag has not produced
yet, so `sha256sums` starts as `SKIP`. The release workflow prints the correct
value in its run summary; immediately after pushing a tag, paste it in and push
a follow-up commit:

```bash
cd packaging/arch && updpkgsums && git commit -am "chore(arch): verify the release tarball"
```

Until that commit lands, this is the only package without source verification.

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
