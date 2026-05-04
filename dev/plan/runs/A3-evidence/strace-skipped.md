# A.3.1 strace tally — skipped

strace not installed on this host and no sudo access to install it.

```
which strace  → not found
apt-get install strace → requires sudo (password-protected)
```

## Implication

Cannot produce `strace-concurrent.txt` with syscall histogram. The
`futex` vs `pread64` split that A.3.1 was meant to provide is
unavailable from this run.

## Workaround evidence

The A.3.2 counter data (counters.json) provides a proxy:

- `embedder_us_total = 23` (RoutedEmbedder is ~0 µs — no I/O, no sleep)
- `search_us_total = 867569 µs` for 1600 concurrent queries
- `proxy_borrow_plus_read_us_per_query = 542 µs`

Since the embedder is instant, essentially all search latency comes from
`ReaderPool::borrow` + `read_search_in_tx`. The dominance of
mutex_atomic in A.2 flamegraphs (5.73× growth) already established that
the contention is mutex-side, not I/O-side. The strace would have
corroborated this via `futex` share vs `pread64` share.

## A.4 input

Mark strace as `skipped:no-strace-available`. Treat A.2 mutex_atomic
verdict as primary evidence. A.3 corroborates via counters only.
