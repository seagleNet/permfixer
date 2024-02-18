# permfixer

A little tool that watches directories and makes sure all files and directories are in line with the configured owner, group and mode.

Configuration can be defined in toml format and passed via command argument.

Config file example:

```toml
[[perm_mapping]]
path = "/var/opt/transfer/input"
uid = 1001
gid = 1001
fmode = 0o640
dmode = 0o750

[[perm_mapping]]
path = "/var/opt/share"
uid = 1002
gid = 1002
fmode = 0o600
dmode = 0o700
```

Command example:

```bash
./permfixer config.toml
```
