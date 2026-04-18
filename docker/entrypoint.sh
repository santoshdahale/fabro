#!/bin/sh
set -eu

# When started as root (the default), ensure the storage volume is writable
# by the unprivileged fabro user, then drop privileges.
if [ "$(id -u)" = 0 ]; then
    chown fabro:fabro /storage
    exec runuser -u fabro -- "$@"
fi

exec "$@"
