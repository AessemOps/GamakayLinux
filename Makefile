# GamakayLinux
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
DESTDIR ?=
UDEVDIR ?= /etc/udev/rules.d

# Packaging. VERSION is the workspace Cargo.toml version, which
# scripts/check-release-version.sh keeps in step with the PKGBUILD and the tag.
VERSION      ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# nfpm and appimagetool are not in every base system; both are released as
# static binaries (see docs/packaging.md) and can be overridden from the
# environment, e.g. NFPM=./tools/nfpm.
NFPM         ?= nfpm
APPIMAGETOOL ?= appimagetool
DEB_ARCH     ?= amd64
RPM_ARCH     ?= x86_64
APPIMAGE_ARCH ?= x86_64
DIST         ?= dist

.PHONY: build release test lint fmt check udev-install install clean
.PHONY: icons package-deb package-rpm package-arch package-appimage packages package-verify package-clean

build:
	cargo build

release:
	cargo build --release --locked

test:
	cargo test

fmt:
	cargo fmt --all

check: fmt
	cargo clippy --all-targets -- -D warnings
	cargo test

lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

# Grant the active local user access to the keyboard's hidraw node.
udev-install:
	install -Dm644 packaging/udev/70-gamakay.rules $(DESTDIR)$(UDEVDIR)/70-gamakay.rules
	udevadm control --reload
	udevadm trigger --subsystem-match=hidraw
	@echo "Replug the keyboard, then run: gamakay info"

install: release
	install -Dm755 target/release/gamakay $(DESTDIR)$(BINDIR)/gamakay
	install -Dm755 target/release/gamakay-gui $(DESTDIR)$(BINDIR)/gamakay-gui
	install -Dm644 packaging/udev/70-gamakay.rules $(DESTDIR)$(UDEVDIR)/70-gamakay.rules
	install -Dm644 packaging/gamakay.desktop \
		$(DESTDIR)$(PREFIX)/share/applications/gamakay.desktop
	for size in 16 24 32 48 64 128 256 512; do \
		install -Dm644 "packaging/icons/hicolor/$${size}x$${size}/apps/gamakay.png" \
			"$(DESTDIR)$(PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/gamakay.png"; \
	done

clean:
	cargo clean

# The icons are committed; this only regenerates them from the SVG.
icons:
	./scripts/make-icons.sh

# --- packages ----------------------------------------------------------------
#
# Every format installs the same files; what differs is the metadata each one
# needs. The build is `--locked` everywhere so a package matches the lockfile.

package-deb: release
	mkdir -p $(DIST)
	VERSION=$(VERSION) ARCH=$(DEB_ARCH) $(NFPM) package \
		--packager deb --config packaging/nfpm.yaml --target $(DIST)/
	@echo "-> $(DIST)/gamakaylinux_$(VERSION)_$(DEB_ARCH).deb"

package-rpm: release
	mkdir -p $(DIST)
	VERSION=$(VERSION) ARCH=$(RPM_ARCH) $(NFPM) package \
		--packager rpm --config packaging/nfpm.yaml --target $(DIST)/
	@echo "-> $(DIST)/gamakaylinux-$(VERSION)-1.$(RPM_ARCH).rpm"

package-arch:
	cd packaging/arch && makepkg -f

package-appimage: release
	rm -rf $(DIST)/AppDir
	install -Dm755 target/release/gamakay-gui $(DIST)/AppDir/usr/bin/gamakay-gui
	install -Dm755 target/release/gamakay $(DIST)/AppDir/usr/bin/gamakay
	install -Dm755 packaging/appimage/AppRun $(DIST)/AppDir/AppRun
	install -Dm644 packaging/gamakay.desktop $(DIST)/AppDir/gamakay.desktop
	install -Dm644 packaging/gamakay.desktop \
		$(DIST)/AppDir/usr/share/applications/gamakay.desktop
	install -Dm644 packaging/io.github.aessemops.gamakaylinux.metainfo.xml \
		$(DIST)/AppDir/usr/share/metainfo/io.github.aessemops.gamakaylinux.metainfo.xml
	install -Dm644 packaging/icons/gamakay.png $(DIST)/AppDir/gamakay.png
	install -Dm644 packaging/icons/gamakay.png \
		$(DIST)/AppDir/usr/share/icons/hicolor/256x256/apps/gamakay.png
	install -Dm644 packaging/udev/70-gamakay.rules \
		$(DIST)/AppDir/usr/share/udev/70-gamakay.rules
	install -Dm644 LICENSE $(DIST)/AppDir/usr/share/doc/gamakaylinux/LICENSE
	install -Dm644 CREDITS.md $(DIST)/AppDir/usr/share/doc/gamakaylinux/CREDITS.md
	install -Dm644 README.md $(DIST)/AppDir/usr/share/doc/gamakaylinux/README.md
	install -Dm644 CHANGELOG.md $(DIST)/AppDir/usr/share/doc/gamakaylinux/CHANGELOG.md
	install -Dm644 crates/gamakay-gui/assets/NotoEmoji-OFL.txt \
		$(DIST)/AppDir/usr/share/doc/gamakaylinux/NotoEmoji-OFL.txt
	ARCH=$(APPIMAGE_ARCH) $(APPIMAGETOOL) $(DIST)/AppDir \
		$(DIST)/GamakayLinux-$(VERSION)-$(APPIMAGE_ARCH).AppImage
	@echo "-> $(DIST)/GamakayLinux-$(VERSION)-$(APPIMAGE_ARCH).AppImage"

packages: package-deb package-rpm package-appimage
	@echo "deb, rpm and AppImage are in $(DIST)/; Arch is built by package-arch"

# List what actually landed in the packages, so the phase is verified against
# the artefacts and not against the config. Neither `dpkg-deb` nor an rpmdb is
# needed: `ar` and `tar` read a .deb, and rpm queries with a scratch db path so
# it does not complain about having no database on a non-rpm host.
RPMDB ?= $(CURDIR)/dist/.rpmdb
package-verify:
	@ls -l $(DIST) | sed 's/^/  /'
	@echo "--- deb: control --------------------------------------------"
	@ar p $(DIST)/gamakaylinux_$(VERSION)_$(DEB_ARCH).deb control.tar.gz \
		| tar xzO ./control | sed 's/^/  /'
	@echo "--- deb: files ----------------------------------------------"
	@ar p $(DIST)/gamakaylinux_$(VERSION)_$(DEB_ARCH).deb data.tar.gz \
		| tar tz | rg 'usr/(bin|lib/udev|share/applications)' | sed 's/^/  /'
	@echo "--- rpm: metadata -------------------------------------------"
	@mkdir -p $(RPMDB)
	@rpm -qpi --dbpath $(RPMDB) $(DIST)/gamakaylinux-$(VERSION)-1.$(RPM_ARCH).rpm \
		| sed -n '1,10p' | sed 's/^/  /'
	@echo "--- rpm: files ----------------------------------------------"
	@rpm -qpl --dbpath $(RPMDB) $(DIST)/gamakaylinux-$(VERSION)-1.$(RPM_ARCH).rpm \
		| sed 's/^/  /'

package-clean:
	rm -rf $(DIST)
