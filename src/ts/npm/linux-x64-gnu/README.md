# fathomdb-linux-x64-gnu

**You probably want [`fathomdb`](https://www.npmjs.com/package/fathomdb) instead.**

```bash
npm install fathomdb
```

This 0.8.20 package contains nothing but the prebuilt **linux-x64-gnu** native binary for FathomDB. It is
declared as an optional dependency of the `fathomdb` package and carries `os`, `cpu` and `libc`
constraints, so npm installs it only on a matching host (Linux, x86-64, glibc) and skips it
everywhere else. The `fathomdb` package's loader picks it up automatically.

There is no JavaScript API here. Installing it directly does nothing useful.

## License

MIT. See the `LICENSE` file shipped in this package.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
