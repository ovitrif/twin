#!/bin/sh
# Make portrait and landscape test videos at iPhone Air proportions (half
# size), for `iOS Mirror --replay` without a phone.
set -eu
out=${1:-build/test}
mkdir -p "$out"
command ffmpeg -loglevel error -y -f lavfi -i "testsrc2=size=630x1368:rate=60:duration=4" -c:v libx264 -pix_fmt yuv420p "$out/portrait.mp4"
command ffmpeg -loglevel error -y -f lavfi -i "testsrc=size=1368x630:rate=60:duration=2" -c:v libx264 -pix_fmt yuv420p "$out/landscape.mp4"
echo "$out/portrait.mp4,$out/landscape.mp4"
