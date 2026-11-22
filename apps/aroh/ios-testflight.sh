#!/bin/sh
exec sh "$(cd "$(dirname "$0")" && pwd)/ios-release.sh" testflight
