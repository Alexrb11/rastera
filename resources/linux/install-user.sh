#!/bin/sh
set -eu
task_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec python3 "$task_directory/install-user.py" "$@"
