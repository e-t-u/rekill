# rekill(1)

A command to kill and restart server processes regularly.
Older server processes often leak memory or other resources. Therefore sometimes you need to kill and restart those servers regularly. rekill is a tool to make restart automatically.

## Usage

```console
$ rekill --help
...
$ rekill --time=10 sleep 20
...(will continue forever, restarts once per 10 seconds)
$ rekill --time=10 sleep 5
Command finished before restart timeout, if you want to restart it, add --restart
$ rekill --time=10 --restart sleep 5
...(will continue forever, restarts once per 5 seconds)
$ rekill -v -v --time=20 sh -c 'echo "hello"; sleep 10; echo "world"'
...
```

## Installation

```console
$ cargo install --git https://github.com/e-t-u/rekill
...
```

Currently, the program is guaranteed to work on Linux only. It may work in Windows and MacOS, but it is not extensively tested.

Program is written with Rust and it requires the daily toolchain.

## --restart

It may happen that the server process terminates before the next restart. By default, rekill exits in this case. If you want to restart the server anyway, use the `--restart` flag.

## --time

The time between restarts can be specified with the `--time` flag. Time is given in seconds. There is no default value.

## Method to kill the process

Server is killed by SIGKILL signal. This means that the process does not get any warning to prepare for termination.

## --verbose and --quiet

By default, the program reports restarts, end of command, and errors. These messages go to stdout.

With --quiet the program does not write normal messages.

With `--verbose` (or `-v`), the program reports some more details. If you use `--verbose`  flag twice, you get debug-level messages. These messages go to stderr.

Combination of `--quiet` and `--verbose` prints only those messages that go to stderr.

## TODO

- Soft kill with SIGTERM first (non portable).
- Support for script to run before restart to clean files etc.
- test suite
- option to daemonize the rekill process, i.e. detach from the terminal
- giving time in minutes or hours
- downgrade requirement of rust to stable
- there should be maximum for restarts in a certain time period
