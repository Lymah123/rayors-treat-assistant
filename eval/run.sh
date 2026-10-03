#!/usr/bin/env bash
cd "$(dirname "$0")/../backend" || exit 1
cargo build --quiet 2>/dev/null
pass=0; total=0
while IFS='|' read -r q expected; do
  [ -z "$q" ] && continue
  total=$((total+1))
  out=$(cargo run --quiet -- "$q" 2>/dev/null)
  if [[ "$out" == *"$expected"* ]]; then
    pass=$((pass+1)); echo "PASS  $q"
  else
    echo "FAIL  $q"; echo "      expected: $expected"; echo "      got:      $out"
  fi
done < ../eval/questions.txt
echo "$pass/$total passed"
