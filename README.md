# rekill(1)

A command to kill and restart server processes regularly.
Older server processes often leak memory or other resources. Therefore sometimes you need to kill and restart those servers regularly. rekill is a tool to make restart automatically.

## Usage

```console
$ rekill --help
...
$ rekill --time=10 --restart leaking-server
...
$ rekill -v -v --time=20 sh -c 'echo "hello"; sleep 10; echo "world"'
...
```

## Installation

```console
$ cargo install --git https://github.com/e-t-u/rekill
...
```

Currently, the program is guaranteed to work on Linux only. Program is written with Rust and it requires the daily toolchain.

## --restart

It may happen that the server process terminates before the next restart. By default, rekill exits in this case. If you want to restart the server anyway, use the `--restart` flag.

## --time

The time between restarts can be specified with the `--time` flag. Time is given in seconds. There is  no default value.

## Method to kill the process

Server is currently killed by SIGKILL signal. This means that the process does not get any warning to prepare for termination.

## --verbose

With `--verbose`, the program reports some details of  use the `--verbose` (or `-v`) flag twice to get debug-level messages.
By default, the program reports restarts and errors.

## TODO

- Soft kill with SIGTERM first.
- Support for other operating systems.
- Mode to watch process and restart (no timeout)
- Support for script to run before restart to clean files etc.
- test suite
- option to daemonize the rekill process, i.e. detach from the terminal
- giving time in minutes as hours
- making `--verbose` messages more orderly and add `--quiet`
- downgrade requirement of rust to stable
