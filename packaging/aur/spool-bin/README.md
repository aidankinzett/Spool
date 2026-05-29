# AUR: `spool-bin`

Binary AUR package for Spool. Installs the raw `spool` binary from the GitHub
release and pulls the runtime tools (`umu-launcher`, `ludusavi`, `rclone`) and
WebKitGTK libs as dependencies, so `paru -S spool-bin` is a one-shot install.

The package consumes the `spool-<version>-x86_64.tar.gz` asset published by
`.github/workflows/release.yml` (binary links against system libs — no AppImage
bundling).

## Per-release maintenance

1. Bump `pkgver` in `PKGBUILD` to the released tag (without the leading `v`); reset `pkgrel=1`.
2. Refresh the checksum: `updpkgsums` (needs `pacman-contrib`).
3. Regenerate metadata: `makepkg --printsrcinfo > .SRCINFO`.
4. Smoke-test locally: `makepkg -si`.
5. Publish to the AUR git repo:
   ```sh
   git clone ssh://aur@aur.archlinux.org/spool-bin.git
   cp PKGBUILD .SRCINFO spool-bin/
   cd spool-bin && git commit -am "spool-bin <version>" && git push
   ```

`PKGBUILD` and `.SRCINFO` are kept in-repo as the source of truth; the AUR repo
is a mirror of these two files.
