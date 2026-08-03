# fathomdb-linux-arm64-gnu

**You probably want [`fathomdb`](https://www.npmjs.com/package/fathomdb) instead.**

```bash
npm install fathomdb
```

This package contains nothing but the prebuilt **linux-arm64-gnu** native binary for FathomDB. It
is declared as an optional dependency of the `fathomdb` package and carries `os`, `cpu` and `libc`
constraints, so npm installs it only on a matching host (Linux, AArch64, glibc) and skips it
everywhere else. The `fathomdb` package's loader picks it up automatically.

There is no JavaScript API here. Installing it directly does nothing useful.

## License

MIT. See the `LICENSE` file shipped in this package.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
