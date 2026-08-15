#!/bin/sh
# Fetch the listening-rig assets: GeneralUser GS and its demo MIDIs.
# Pinned to the upstream repo; assets/ stays out of git.
set -e
cd "$(dirname "$0")/../assets"
BASE="https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/main"
[ -f GeneralUser-GS.sf2 ] || curl -LfsS -o GeneralUser-GS.sf2 "$BASE/GeneralUser-GS.sf2"
for m in "Bond.mid" "Breakout.mid" "Dance.mid" "J-cycle.mid" "Jump!.mid" \
         "Umi no Mieru Machi.mid" "The HYBRID Collage (v2.0) - by S. Christian Collins.mid"; do
  [ -f "$m" ] || curl -LfsS -o "$m" "$BASE/demo%20MIDIs/$(printf %s "$m" | sed 's/ /%20/g')"
done
echo "assets ready"
