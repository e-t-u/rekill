# rekill(1)

A command to kill and restart server processes regularly.

Older server processes often leak memory or other resources. Therefore sometimes you need to kill and restart those servers regularly. rekill is a tool to make restart automatically.

## Usage

```console
$ rekill --help
$ rekill --time=10 --restart leaking-server
$ rekill -v -v --time=20 sh -c 'echo "hello"; sleep 10; echo "world"'
```

## Installation

```console
$ cargo install --git https://github.com/e-t-u/rekill
```

Currently, the program is guaranteed to work on Linux only. Program is written with Rust.

## --restart

It may happen that the server process terminates before the next restart. By default, rekill exits in this case. If you want to restart the server anyway, use the `--restart` flag.

## --time

The time between restarts can be specified with the `--time` flag. There is  no default value.

## Method to kill the process

Server is currently killed by SIGKILL. This may change in the future.
