#!/bin/sh
# Boot the hosted kernel; init (pid 1) runs the acceptance tests.
cd "$(dirname "$0")"
exec node supervisor/main.mjs rootfs
