# Packaging manifests

This directory holds the package-manager manifests for `rusty-sched`. None of
these are consumed by `cargo` or the build itself — they are pushed (or PR'd)
to external package repositories after a tagged release uploads its artifacts.

## Layout

| Path                            | Purpose                                                                                  |
| ------------------------------- | ---------------------------------------------------------------------------------------- |
| `homebrew/rusty-sched.rb`       | Homebrew Formula. Pulls the prebuilt macOS tarball from the GitHub Release by URL+SHA256. |
| `winget/rusty-sched.yaml`       | winget v1.6 manifest. References the signed (or unsigned) MSI from the GitHub Release.   |

The MSI source (`installers/windows/rusty-sched.wxs`), the `.pkg` (built
in-workflow via `pkgbuild`), and the `.deb` / `.rpm` (built via `cargo-deb` and
`cargo-generate-rpm`) all live outside this directory; they're built and
attached to the GitHub Release by `.github/workflows/release.yml`.

## Release flow

1. Push tag `vX.Y.Z`. `release.yml` builds binaries, `.pkg`, `.msi`, `.deb`,
   `.rpm` and uploads them to the GitHub Release.
2. **Homebrew**: bump `version` + `sha256` in `homebrew/rusty-sched.rb`, then
   copy it to the separate `homebrew-rusty-sched` tap repo on its `main`
   branch — `brew install jdp5949/rusty-sched/rusty-sched` works after that.
3. **winget**: bump `PackageVersion` + `InstallerSha256` + `InstallerUrl` in
   `winget/rusty-sched.yaml`, then open a PR to
   [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs) under
   `manifests/r/rusty-sched/rusty-sched/<version>/`.

Both steps are intentionally manual — they require the published release SHAs
which only exist after the release job finishes.
