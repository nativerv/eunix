#!/bin/sh

head -c "${1}" < /dev/zero > "${2}"
