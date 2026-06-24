// Build the data blob for the wordlist curation reviewer.
// Tags each Union <=6 word with: source list(s) (S/B/L), length, possible
// inflection, real-world frequency rank (hermitdave en_50k), and a "proposed
// exclusion" flag = my recommendation to drop it.
//
// Proposal logic: Short #1 and BIP39 words are already vetted (common, clean) ->
// never proposed. Among Large-only words, propose dropping the ones that are rare
// (absent from the 50k frequency list, or ranked beyond a threshold) or are
// redundant inflections of another list word. Reviewer can override every call.
//
// Usage: node build-curate-data.js [out.json]   (no arg -> just prints stats)
const fs = require("fs");
const T = process.env.TMPDIR;
const RARE_RANK = 40000; // beyond this (or absent from top-50k) a Large-only word is "rare" enough to propose dropping

function rd(f, col) {
  return fs.readFileSync(f, "utf8").trim().split("\n")
    .map(l => { const p = l.split("\t"); return (col ? p[1] : p[0]).trim().toLowerCase(); })
    .filter(Boolean);
}

const short1 = new Set(rd("core/src/eff_short.txt", 0));
const bip39  = new Set(rd(T + "/bip39.txt", 0));
const large  = new Set(rd(T + "/eff_large_raw.txt", 1));
const words  = rd("tools/yuiolink-union-le6.txt", 0);
const wset   = new Set(words);

const freq = new Map();
fs.readFileSync(T + "/en_50k.txt", "utf8").trim().split("\n").forEach((l, i) => {
  const w = l.split(" ")[0].toLowerCase();
  if (w && !freq.has(w)) freq.set(w, i + 1);
});

function inflectionOf(w) {
  const cuts = [["s", 1], ["es", 2], ["ed", 2], ["d", 1], ["ing", 3], ["er", 2], ["r", 1]];
  for (const [suf, n] of cuts) {
    if (w.endsWith(suf) && w.length - n >= 3) {
      const base = w.slice(0, w.length - n);
      if (wset.has(base)) return base;
      if (suf === "ing" || suf === "ed") { const baseE = base + "e"; if (wset.has(baseE)) return baseE; }
    }
  }
  return null;
}

const data = words.map(w => {
  let src = "";
  if (short1.has(w)) src += "S";
  if (bip39.has(w))  src += "B";
  if (large.has(w))  src += "L";
  const infl = inflectionOf(w);
  const rank = freq.get(w) || null;
  const largeOnly = src === "L";
  const rare = rank == null || rank > RARE_RANK;
  // Why I'd propose dropping it (empty array = keep):
  const why = [];
  if (largeOnly && rare) why.push(rank == null ? "not in the 50k most-common words" : ("uncommon — ranked #" + rank.toLocaleString()));
  if (infl) why.push("looks like “" + infl + "” + a suffix");
  const proposed = largeOnly && rare;
  return { w, src, len: w.length, infl, rank, proposed, why };
});

const proposedList = data.filter(d => d.proposed);
const stats = {
  total: data.length,
  largeOnly: data.filter(d => d.src === "L").length,
  proposed: proposedList.length,
  proposedRare: data.filter(d => d.proposed && (d.rank == null || d.rank > RARE_RANK)).length,
  proposedInfl: data.filter(d => d.proposed && d.infl).length,
};

const out = process.argv[2];
console.error(JSON.stringify(stats));
console.error("sample proposed: " + proposedList.slice(0, 30).map(d => d.w).join(", "));
if (out) { fs.writeFileSync(out, JSON.stringify(data)); console.error("wrote " + data.length + " records to " + out); }
