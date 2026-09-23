#!/bin/sh
cd -- "$(dirname -- "$0")" || exit 1
if [ -x ./vanitybtc ]; then
  vanity_binary=./vanitybtc
elif [ -x ./target/release/vanitybtc ]; then
  vanity_binary=./target/release/vanitybtc
else
  printf 'Build the app once with: cargo build --release --locked --offline\n'
  printf 'Press Enter to close. '
  read -r answer || true
  exit 1
fi
"$vanity_binary" --interactive
vanity_status=$?
printf '\nPress Enter to close. '
read -r answer || true
exit "$vanity_status"
