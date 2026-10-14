# GamakayLinux
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
DESTDIR ?=
UDEVDIR ?= /etc/udev/rules.d

.PHONY: build release test lint fmt check udev-install install clean

build:
	cargo build

release:
	cargo build --release

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
	install -Dm644 packaging/udev/70-gamakay.rules $(DESTDIR)$(UDEVDIR)/70-gamakay.rules

clean:
	cargo clean
