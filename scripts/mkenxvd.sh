#!/bin/sh

head -c "${1}" < /dev/random > "${2}"
