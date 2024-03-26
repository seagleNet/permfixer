# permfixer

A little program that watches directories recursively and makes sure all files and directories are in line with the configured owner, group and mode. It's been written for Linux specifically and does not work on any other OS.

The program doesn't stop until it's killed or if all watched directories have been deleted. It can also be started as a systemd service - see the example below.

Logs are written to stdout/stderr.

## Usage

Since the program needs to be able to execute `chown` it needs to be run as root.

```bash
permfixer <config file>
```

Examples:

```bash
# Running in foreground
sudo /bin/permfixer /etc/permfixer.toml
# Running in background
nohup sudo /bin/permfixer /etc/permfixer.toml &
```

When you want to stop the program simply kill it:

```bash
sudo pkill permfixer
```

## Configuration

Configuration needs to be defined in a toml file and passed via command argument.

An example of a config could look something like this:

```toml
[[perm_mapping]]                  # Config array
path = "/var/opt/transfer/input"  # Path to watch recursively
uid = 1001                        # User's ID for chown
gid = 1001                        # Group's ID for chgrp
fmode = 0o640                     # Permissions in octal format for files
dmode = 0o750                     # Permissions in octal format for direcotires

[[perm_mapping]]
path = "/var/opt/share"
uid = 1002
gid = 1002
fmode = 0o600
dmode = 0o700
```

## Installation

Make sure you have the cargo and all the prerequisites to build rust applications installed on your system. Then simply run the following commands to build and install permfixer:

```bash
cargo install --git https://git.seagle.sh/seagle/permfixer.git
sudo install -v -o root -g root -m 755 ~/.cargo/bin/permfixer /bin
```

## Systemd unit example

An example of a systemd service file.

```ini
[Unit]
Description=Run permfixer

[Service]
User=root
ExecStart=/path/to/permfixer /path/to/config.toml

[Install]
WantedBy=multi-user.target
```

For further information check the man pages of `systemd`, `systemd.unit`, `systemd.service` etc.

## Disclaimer

I tested this software as thoroughly as possible, however, there could still be issues I might have missed. Make sure your config file makes sense and that you only target directories you intend to. There could be undesired consequences or you could even brick your system if you're not being careful.

Use this program at your own risk.
