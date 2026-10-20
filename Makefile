# lk67ctl
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
# The runtime appimagetool embeds. Unset, appimagetool downloads it from a
# rolling `continuous` URL at build time, unverified; set it to a file you have
# checked (see docs/packaging.md) and it is used instead.
APPIMAGE_RUNTIME ?=
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

check: lint
	cargo test --all --locked

lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

# Grant the active local user access to the keyboard's hidraw node.
udev-install:
	install -Dm644 packaging/udev/70-lk67ctl.rules $(DESTDIR)$(UDEVDIR)/70-lk67ctl.rules
	udevadm control --reload
	udevadm trigger --subsystem-match=hidraw
	@echo "Replug the keyboard, then run: lk67ctl info"

install: release
	install -Dm755 target/release/lk67ctl $(DESTDIR)$(BINDIR)/lk67ctl
	install -Dm755 target/release/lk67-gui $(DESTDIR)$(BINDIR)/lk67-gui
	install -Dm644 packaging/udev/70-lk67ctl.rules $(DESTDIR)$(UDEVDIR)/70-lk67ctl.rules
	install -Dm644 packaging/io.github.aessemops.lk67ctl.metainfo.xml \
		$(DESTDIR)$(PREFIX)/share/metainfo/io.github.aessemops.lk67ctl.metainfo.xml
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/licenses/lk67ctl/LICENSE
	install -Dm644 CREDITS.md $(DESTDIR)$(PREFIX)/share/licenses/lk67ctl/CREDITS.md
	install -Dm644 crates/lk67-gui/assets/NotoEmoji-OFL.txt \
		$(DESTDIR)$(PREFIX)/share/licenses/lk67ctl/NotoEmoji-OFL.txt
	install -Dm644 packaging/lk67ctl.desktop \
		$(DESTDIR)$(PREFIX)/share/applications/lk67ctl.desktop
	for size in 16 24 32 48 64 128 256 512; do \
		install -Dm644 "packaging/icons/hicolor/$${size}x$${size}/apps/lk67ctl.png" \
			"$(DESTDIR)$(PREFIX)/share/icons/hicolor/$${size}x$${size}/apps/lk67ctl.png"; \
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
	@echo "-> $(DIST)/lk67ctl_$(VERSION)_$(DEB_ARCH).deb"

package-rpm: release
	mkdir -p $(DIST)
	VERSION=$(VERSION) ARCH=$(RPM_ARCH) $(NFPM) package \
		--packager rpm --config packaging/nfpm.yaml --target $(DIST)/
	@echo "-> $(DIST)/lk67ctl-$(VERSION)-1.$(RPM_ARCH).rpm"

package-arch:
	cd packaging/arch && makepkg -f

package-appimage: release
	rm -rf $(DIST)/AppDir
	install -Dm755 target/release/lk67-gui $(DIST)/AppDir/usr/bin/lk67-gui
	install -Dm755 target/release/lk67ctl $(DIST)/AppDir/usr/bin/lk67ctl
	install -Dm755 packaging/appimage/AppRun $(DIST)/AppDir/AppRun
	install -Dm644 packaging/lk67ctl.desktop $(DIST)/AppDir/lk67ctl.desktop
	install -Dm644 packaging/lk67ctl.desktop \
		$(DIST)/AppDir/usr/share/applications/lk67ctl.desktop
	install -Dm644 packaging/io.github.aessemops.lk67ctl.metainfo.xml \
		$(DIST)/AppDir/usr/share/metainfo/io.github.aessemops.lk67ctl.metainfo.xml
	install -Dm644 packaging/icons/lk67ctl.png $(DIST)/AppDir/lk67ctl.png
	install -Dm644 packaging/icons/lk67ctl.png \
		$(DIST)/AppDir/usr/share/icons/hicolor/256x256/apps/lk67ctl.png
	install -Dm644 packaging/udev/70-lk67ctl.rules \
		$(DIST)/AppDir/usr/share/udev/70-lk67ctl.rules
	install -Dm644 LICENSE $(DIST)/AppDir/usr/share/doc/lk67ctl/LICENSE
	install -Dm644 CREDITS.md $(DIST)/AppDir/usr/share/doc/lk67ctl/CREDITS.md
	install -Dm644 README.md $(DIST)/AppDir/usr/share/doc/lk67ctl/README.md
	install -Dm644 CHANGELOG.md $(DIST)/AppDir/usr/share/doc/lk67ctl/CHANGELOG.md
	install -Dm644 crates/lk67-gui/assets/NotoEmoji-OFL.txt \
		$(DIST)/AppDir/usr/share/doc/lk67ctl/NotoEmoji-OFL.txt
	ARCH=$(APPIMAGE_ARCH) $(APPIMAGETOOL) \
		$(if $(APPIMAGE_RUNTIME),--runtime-file $(APPIMAGE_RUNTIME),) \
		$(DIST)/AppDir $(DIST)/lk67ctl-$(VERSION)-$(APPIMAGE_ARCH).AppImage
	@echo "-> $(DIST)/lk67ctl-$(VERSION)-$(APPIMAGE_ARCH).AppImage"

packages: package-deb package-rpm package-appimage
	@echo "deb, rpm and AppImage are in $(DIST)/; Arch is built by package-arch"

# List what actually landed in the packages, so the phase is verified against
# the artefacts and not against the config. Neither `dpkg-deb` nor an rpmdb is
# needed: `ar` and `tar` read a .deb, and rpm queries with a scratch db path so
# it does not complain about having no database on a non-rpm host.
RPMDB ?= $(CURDIR)/dist/.rpmdb
# Needs rpm and rg (see docs/packaging.md).
package-verify:
	@ls -l $(DIST) | sed 's/^/  /'
	@echo "--- deb: control --------------------------------------------"
	@ar p $(DIST)/lk67ctl_$(VERSION)_$(DEB_ARCH).deb control.tar.gz \
		| tar xzO ./control | sed 's/^/  /'
	@echo "--- deb: files ----------------------------------------------"
	@ar p $(DIST)/lk67ctl_$(VERSION)_$(DEB_ARCH).deb data.tar.gz \
		| tar tz | rg 'usr/(bin|lib/udev|share/applications)' | sed 's/^/  /'
	@echo "--- rpm: metadata -------------------------------------------"
	@mkdir -p $(RPMDB)
	@rpm -qpi --dbpath $(RPMDB) $(DIST)/lk67ctl-$(VERSION)-1.$(RPM_ARCH).rpm \
		| sed -n '1,10p' | sed 's/^/  /'
	@echo "--- rpm: files ----------------------------------------------"
	@rpm -qpl --dbpath $(RPMDB) $(DIST)/lk67ctl-$(VERSION)-1.$(RPM_ARCH).rpm \
		| sed 's/^/  /'

package-clean:
	rm -rf $(DIST)
