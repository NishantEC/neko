## Killing an evidence client: `pkill -x neko` is wrong, and so is `pkill -f`

Recorded because it has now gone wrong twice, in opposite directions, and both
failures are silent.

- **`pkill -f '/tmp/<dir>/bin/neko'` matches nothing.** `ps` reports the command
  as the *relative* `./neko`, because that is how the process was launched, so
  the absolute path never appears in the string `-f` searches. Nothing dies, the
  command succeeds, and the strays accumulate — nine of them across one session,
  each holding an invisible non-activating window and competing for CPU with
  whatever is being measured next. **Check the exit code, or better, check
  `pgrep` afterwards.**
- **`pkill -x neko` matches too much** — including the captain's own running
  client, which is also called `neko`. Done once here; the client was restarted
  immediately, and the daemon survived by design.

**The reliable identification is the working directory**, which is the isolated
`HOME` the run was launched from:

```sh
for p in $(pgrep -x neko); do
  printf "%-7s " $p; lsof -a -p $p -d cwd -Fn 2>/dev/null | grep '^n'
done
```

Kill by pid, only where the cwd is the temp directory of the run being cleaned
up. The captain's own client shows a cwd of the repo itself.

**And this is what noisy measurements look like.** Idle CPU for this app read
5.97%, then 1.67%, then 2.14% across three runs of the same binary — the strays
were the variance. With them gone the same measurement is 0.42% mean / 0.40%
median, reproducibly. A benchmark that will not settle is worth one `pgrep`
before it is worth another theory.
