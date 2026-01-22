#!/usr/bin/env sh

# Script that checks cross-crate doc links to docs.rs and replaces them to local links in the generated documentation.
# This allows serving docs locally or from `github.io` without inks getting stale / leading to nowhere.

set -e

# Returns a relative path of $2 relative to $1.
#
# Copied from https://unix.stackexchange.com/questions/573047/how-to-get-the-relative-path-between-two-directories
relative_path() {
  set -- "${1%/}/" "${2%/}/" ''
  while [ "$1" ] && [ "$2" = "${2#"$1"}" ]; do
    set -- "${1%/?*/}/" "$2" "../$3"
  done
  REPLY="${3}${2#"$1"}" ## build result
  # unless root chomp trailing '/', replace '' with '.'
  [ "${REPLY#/}" ] && REPLY="${REPLY%/}" || REPLY="${REPLY:-.}"
  echo "$REPLY"
}

echo "Checking dependencies..."
which cargo > /dev/null || {
  echo "cargo is missing"
  exit 1
}
which sed > /dev/null || {
  echo "sed is missing"
  exit 1
}
which jq > /dev/null || {
  echo "jq is missing"
  exit 1
}

ROOT_DIR=$(realpath "$(dirname "$0")")

pkg_version=$(
  cargo metadata --format-version=1 --no-deps --manifest-path="$ROOT_DIR/Cargo.toml" \
   | jq -r '.packages.[] | select(.name == "smart-config").version'
)
echo "Read package version: $pkg_version"

# `sed` commands that matches `https://docs.rs/smart-config/$VERSION/` links and outputs $VERSION
SED_VERSION_CMD='s#^.*https://docs\.rs/smart-config/([^/]+)/smart_config.*$#\1#p'

if [ "$1" = "--check" ]; then
  echo "Checking Rust sources for invalid docs.rs links"

  src_files=$(find "$ROOT_DIR/crates" -name '*.rs' -path '*/crates/*/src/*' -print)
  invalid_files=0
  for file in $src_files; do
    echo "Checking file $file..."
    present_versions=$(sed -E -n -e "$SED_VERSION_CMD" "$file" | sort -u)
    if [ "$present_versions" != "" ]; then
      echo "File $file links to docs.rs"
      if [ "$present_versions" != "$pkg_version" ]; then
        echo "File $file has invalid docs.rs links; must use version $pkg_version"
        invalid_files=1
      fi
    fi
  done

  if [ $invalid_files -eq 1 ]; then
    exit 2
  fi
  exit 0
fi

if [ ! -d "$ROOT_DIR/target/doc" ]; then
  echo "No generated docs"
  exit 1
fi

echo "Replacing docs.rs links in generated docs"
html_files=$(find "$ROOT_DIR/target/doc" -name '*.html' -path '*/smart_config*/*' -print)
for file in $html_files; do
  echo "Checking HTML file $file"
  # We want a relative path replacement because it is most versatile; it works regardless of where
  # the docs are served from.
  path_to_config_docs=$(relative_path "$(dirname "$file")" "$ROOT_DIR/target/doc/smart_config")
  sed_replace_cmd='s#https://docs\.rs/smart-config/([^/]+)/smart_config#'"$path_to_config_docs"'#g'
  sed -E -i '' -e "$sed_replace_cmd" "$file"
done
