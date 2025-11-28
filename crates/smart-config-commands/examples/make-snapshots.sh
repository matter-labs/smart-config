#!/bin/sh

set -e

# Common `term-transcript` CLI args
TT_ARGS="-T 1s --pure-svg --palette=ubuntu --width=860 --no-wrap --no-inputs --line-numbers=continuous --window"

THIS_DIR=$(dirname "$0")
ROOT_DIR=$(realpath "$THIS_DIR/../../..")
TARGET_DIR="$ROOT_DIR/target/debug"

echo "Checking term-transcript..."
which term-transcript || cargo install term-transcript-cli --locked --version=0.4.0 --force
term-transcript --version

echo "Building CLI example..."
cargo build -p smart-config-commands --example cli
if [ ! -x "$TARGET_DIR/examples/cli" ]; then
  echo "Example executable not found at expected location"
  exit 1
fi

# Force ANSI coloring despite not writing to a terminal
export CLICOLOR_FORCE=1

echo "Creating help snapshot..."
term-transcript exec $TT_ARGS --scroll=540 "$TARGET_DIR/examples/cli print" > "$THIS_DIR/help.svg"
echo "Creating debug snapshot..."
term-transcript exec $TT_ARGS --scroll=540 "$TARGET_DIR/examples/cli debug" > "$THIS_DIR/debug.svg"
echo "Creating errors snapshot..."
term-transcript exec $TT_ARGS --scroll=540 "$TARGET_DIR/examples/cli debug --bogus" > "$THIS_DIR/errors.svg"
echo "Create YAML serialization snapshot..."
term-transcript exec $TT_ARGS "$TARGET_DIR/examples/cli serialize --diff" > "$THIS_DIR/ser-yaml.svg"
