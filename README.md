# rekill(1)

A command to kill and restart server processes regularly.
Older server processes often leak memory or other resources. Therefore,
sometimes you need to kill and restart those servers regularly.
`rekill` is a tool to make restart automatically.

## Usage

```console
$ rekill --help
...
$ rekill --time=10s sleep 20
...(will continue forever, restarts once per 10 seconds)
$ rekill --time=10s sleep 5
Command finished before restart timeout, if you want to restart it, add --restart
$ rekill --time=10s --restart sleep 5
...(will continue forever, restarts once per 5 seconds)
$ rekill --time=5000ms sh -c 'echo "hello"; sleep 10; echo "world"'
...
$ rekill --time=1h20min3s sh -c 'echo "hello"; sleep 2h; echo "world"'
...
```

## Installation

```console
$ cargo install --git https://github.com/e-t-u/rekill
...
```

Currently, the program is guaranteed to work on Linux only. It may work in Windows and MacOS, but it is not extensively tested.

The program is written with Rust, and it requires the daily toolchain.

## --restart

The server process may terminate before the next restart. By default, rekill exits in this case. If you want to restart the server anyway, use the `--restart` flag.

## --time

The time between restarts can be specified with the `--time` flag. Time is given in human units like 300ms, 1s, 10min, 1h20min1s, '1h 20 min 1s'. There is no default value.

## Method to kill the process

The server is killed by the `SIGKILL` signal. The Server process does not get any warning to prepare for termination.

## --verbose and --quiet

By default, the program reports restarts, end of command, and errors. These messages go to stdout.

With `--quiet`, the program does not write normal messages.

With `--verbose` (or `-v`), the program reports some more details. If you use the `--verbose`  flag twice, you get debug-level messages. These messages go to stderr.

The combination of `--quiet` and `--verbose` prints only those messages that go to stderr.

## TODO

- Soft kill with SIGTERM first (non-portable).
- Support for script to run before restart to clean files, etc.
- Test suite
- Option to daemonize the rekill process, i.e. detach from the terminal
- Downgrade requirement of Rust to stable
- There should be a maximum for restarts in a period to avoid restart loops
