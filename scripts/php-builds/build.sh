#!/usr/bin/env bash
# Builds static PHP (cli + fpm) for macOS with static-php-cli and packs it as
# php-<version>-macos-<arch>.tar.gz: php, php-fpm and the licenses of everything
# compiled in. Used by .github/workflows/php-macos.yml; runs locally too.
#
#   scripts/php-builds/build.sh <version> <source-url> <source-sha256> <arm64|x64> [output-dir]
#
# Needs Xcode command line tools. `spc doctor --auto-fix` installs missing build
# tools (autoconf, bison, …) with Homebrew.
set -euo pipefail

VERSION=$1
SOURCE_URL=$2
SOURCE_SHA256=$3
ARCH=$4
OUTPUT=${5:-$PWD/dist}

HERE=$(cd "$(dirname "$0")" && pwd)
# Pinned static-php-cli release. Bump deliberately: it decides library versions.
SPC_VERSION=2.8.5
LIBS=freetype,libjpeg,libwebp

# Whether line $1 is below line $2 (8.5 < 8.6 < 8.10).
version_below() {
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" = "$1" ]
}
LINE=$(echo "$VERSION" | cut -d. -f1,2)
EXTENSIONS=""
while read -r name limit; do
  case "$name" in "" | "#"*) continue ;; esac
  if [ -n "$limit" ] && ! version_below "$LINE" "${limit#<}"; then
    echo "skipping $name on PHP $LINE (built only below ${limit#<})"
    continue
  fi
  EXTENSIONS="${EXTENSIONS:+$EXTENSIONS,}$name"
done < "$HERE/extensions.txt"

if ! version_below "$LINE" 8.6; then
  # spc's "micro" patches (cli_checks, macos_iconv) target the micro SAPI, which
  # Werd does not build, and no longer apply to PHP 8.6's sources.
  export SPC_MICRO_PATCHES=,
fi

case "$ARCH" in
  arm64) SPC_ARCH=aarch64 ;;
  x64) SPC_ARCH=x86_64 ;;
  *) echo "unknown architecture: $ARCH" >&2; exit 2 ;;
esac
if [ "$(uname -m)" != "$( [ "$ARCH" = arm64 ] && echo arm64 || echo x86_64 )" ]; then
  echo "build $ARCH on a matching Mac (this one is $(uname -m))" >&2
  exit 2
fi

WORK=${WORK:-$(mktemp -d)}
mkdir -p "$WORK" "$OUTPUT"
cd "$WORK"

if [ ! -x spc ]; then
  curl -fsSL -o spc.tar.gz \
    "https://github.com/crazywhalecc/static-php-cli/releases/download/$SPC_VERSION/spc-macos-$SPC_ARCH.tar.gz"
  tar xzf spc.tar.gz
fi
./spc --version
./spc doctor --auto-fix

./spc download \
  --for-extensions="$EXTENSIONS" \
  --for-libs="$LIBS" \
  --custom-url="php-src:$SOURCE_URL" \
  --prefer-pre-built \
  --retry=3

# The PHP source must match the checksum php.net publishes.
SOURCE_FILE=downloads/$(basename "$SOURCE_URL")
echo "$SOURCE_SHA256  $SOURCE_FILE" | shasum -a 256 -c -

./spc build "$EXTENSIONS" --build-cli --build-fpm --with-libs="$LIBS"

# Smoke test: the version, every requested extension, and php-fpm's config parser.
PHP=buildroot/bin/php
FPM=buildroot/bin/php-fpm
"$PHP" -v
"$FPM" -v
LOADED=$("$PHP" -m | tr '[:upper:]' '[:lower:]')
MISSING=""
for extension in ${EXTENSIONS//,/ }; do
  case "$extension" in
    mbregex) continue ;; # part of mbstring, not listed by `php -m`
    opcache) name="zend opcache" ;;
    *) name=$extension ;;
  esac
  grep -qx "$name" <<<"$LOADED" || MISSING="$MISSING $extension"
done
if [ -n "$MISSING" ]; then
  echo "missing extensions:$MISSING" >&2
  exit 1
fi
"$PHP" -r 'foreach (["mysql", "pgsql", "sqlite"] as $driver) { in_array($driver, PDO::getAvailableDrivers(), true) or exit(1); }'
printf '[global]\nerror_log=/dev/stderr\n[www]\nlisten=127.0.0.1:9999\npm=ondemand\npm.max_children=1\n' > fpm-test.conf
"$FPM" -t --fpm-config fpm-test.conf

STAGE=$WORK/package
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "$PHP" "$FPM" "$STAGE/"
if [ -d buildroot/license ]; then
  cp -R buildroot/license "$STAGE/licenses"
fi
{
  echo "PHP $VERSION for macOS $ARCH, built by Werd with static-php-cli $SPC_VERSION."
  echo "Source: $SOURCE_URL (sha256 $SOURCE_SHA256)"
  echo "Extensions: $EXTENSIONS"
  echo "Libraries: $LIBS"
} > "$STAGE/BUILD.txt"

ARCHIVE=$OUTPUT/php-$VERSION-macos-$ARCH.tar.gz
tar -czf "$ARCHIVE" -C "$STAGE" .
(cd "$OUTPUT" && shasum -a 256 "$(basename "$ARCHIVE")" > "$(basename "$ARCHIVE").sha256")
echo "built $ARCHIVE"
