# 0.8.22 npm trusted-publishing bootstrap

Run this once, manually, before a real 0.8.22 tag is pushed. It grants no
long-lived npm token to GitHub.

1. In npm, create or claim these public, unscoped packages: `fathomdb-darwin-x64`,
   `fathomdb-darwin-arm64`, and `fathomdb-win32-x64-msvc`.
2. For each package, configure GitHub Actions trusted publishing for repository
   `coreyt/fathomdb`, workflow `.github/workflows/release.yml`, and the package
   publication environment used by the release workflow.
3. Verify the npm package owner permits provenance and that the GitHub workflow
   has `id-token: write` on each platform-publish job and the promotion job.
4. Run the release workflow as a dry run from the immutable candidate commit;
   confirm every new platform job stages exactly one matching `.node` file.
5. Before the real run, record the npm UI configuration and obtain the normal
   explicit HITL publish authorization. Do not add `NPM_TOKEN` or another
   long-lived npm credential as a repository secret.

The real run publishes every platform package and the thin `fathomdb` package
under `next`. Only `fathomdb@<version>` is promoted to `latest`, and only after
all five actual-runner registry smokes and co-tagging succeed.
