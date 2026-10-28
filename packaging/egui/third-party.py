#!/usr/bin/env python3
"""Inventory of the Rust crates that are compiled into omatree-egui, from the resolved graph.

  third-party.py table            print the Markdown table (stdout)
  third-party.py check FILE       fail unless the generated block in FILE is current
  third-party.py update FILE      rewrite the generated block in FILE
  third-party.py bundle OUT       write the licence bundle shipped in packages (inventory + texts)

The set is `cargo tree -e normal,build` for the omatree-egui binary on x86_64 Linux, i.e. what
the build compiles (proc-macros and build scripts included, dev-dependencies not). Licence
expressions come from each crate's manifest (as `cargo tree` prints them) and licence files from its
own source, and a crate without either is listed as MISSING, never guessed. Needs the crate sources
in the Cargo home (`cargo fetch --locked`); works offline.
"""
import glob, re, subprocess, sys, os, hashlib

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
TARGET = "x86_64-unknown-linux-gnu"
BEGIN, END = "<!-- BEGIN GENERATED: egui-crates -->", "<!-- END GENERATED: egui-crates -->"
LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)([-_.].*)?$", re.I)


def run(*a):
    return subprocess.check_output(a, cwd=ROOT, text=True)


def crate_dir(name, version, src):
    """The crate's source directory in the Cargo home (registry or git checkout), or None."""
    home = os.environ.get("CARGO_HOME") or os.path.expanduser("~/.cargo")
    if src.startswith("http"):  # a git dependency: .../checkouts/<repo>-<hash>/<short rev>
        rev = src.rsplit("#", 1)[-1]
        for d in glob.glob(os.path.join(home, "git", "checkouts", "*", rev[:7] + "*")):
            for cand in (d, *glob.glob(os.path.join(d, "*"))):
                if os.path.isfile(os.path.join(cand, "Cargo.toml")) and f'name = "{name}"' in open(os.path.join(cand, "Cargo.toml")).read():
                    return cand
        return None
    hits = glob.glob(os.path.join(home, "registry", "src", "*", f"{name}-{version}"))
    return hits[0] if hits else None


def graph():
    """(name, version, license, direct, dir, source) for every compiled crate, from `cargo tree`.

    `cargo tree` rather than `cargo metadata`: it needs only the crates this target compiles, so it
    works in a build environment that fetched just those (`cargo fetch --target ...`) and is offline."""
    base = ["cargo", "tree", "--locked", "--offline", "-p", "omatree-egui", "--target", TARGET, "-e", "normal,build",
            "--prefix", "none", "--format", "{p}|{l}"]
    pat = re.compile(r"^(\S+) v(\S+)(?: \(([^)]*)\))?\|(.*?)(?: \(\*\))?$")
    def parse(text):
        out = {}
        for line in text.splitlines():
            m = pat.match(line)
            if m:
                out[(m.group(1), m.group(2))] = (m.group(4).strip(), m.group(3) or "")
        return out
    used = parse(run(*base))
    direct = {k[0] for k in parse(run(*base, "--depth", "1"))}
    rows = []
    for (name, version), (lic, src) in sorted(used.items()):
        if name in ("omatree-egui", "omatree-core"):
            continue
        d = crate_dir(name, version, src)
        if d is None:
            sys.exit(f"third-party.py: no source for {name} {version} in the Cargo home; run `cargo fetch --locked` first")
        rows.append(dict(name=name, version=version, license=lic or "MISSING", direct=name in direct, path=d,
                         source="git (pinned rev)" if src.startswith("http") else "crates.io"))
    return rows


def table(rows):
    out = [f"{len(rows)} crates are compiled into `omatree-egui` (excluding `omatree-egui` and `omatree-core`, which are OmaTree's own); "
           f"{sum(r['direct'] for r in rows)} are direct dependencies (**bold**).", "",
           "| Crate | Version | License expression | Source |", "|---|---|---|---|"]
    for r in rows:
        n = f"**{r['name']}**" if r["direct"] else r["name"]
        out.append(f"| {n} | {r['version']} | {r['license'].replace('|', '/')} | {r['source']} |")
    return "\n".join(out)


def block(rows):
    return f"{BEGIN}\n{table(rows)}\n{END}"


# Crates that declare a licence but ship no licence file: the upstream copy kept in packaging/egui/licences
# (packaging/egui/licences/SOURCES.md says where each came from). Keyed by crate name.
UPSTREAM = {**{n: ("egui", ["egui-LICENSE-MIT", "egui-LICENSE-APACHE"]) for n in (
    "egui", "eframe", "epaint", "emath", "ecolor", "egui-winit", "egui_glow", "egui_system_font_provider")},
    **{n: ("accesskit", ["accesskit-LICENSE-MIT", "accesskit-LICENSE-APACHE"]) for n in (
    "accesskit", "accesskit_consumer", "accesskit_atspi_common", "accesskit_unix", "accesskit_winit")},
    "krilla": ("krilla", ["krilla-LICENSE_MIT", "krilla-LICENSE_APACHE"]),
    "profiling": ("profiling", ["profiling-LICENSE-MIT", "profiling-LICENSE-APACHE"]),
    "gl_generator": ("gl-rs", ["gl-rs-LICENSE"]), "khronos_api": ("gl-rs", ["gl-rs-LICENSE"])}
UPSTREAM_DIR = os.path.join(ROOT, "packaging", "egui", "licences")


def licence_files(path):
    return sorted(f for f in os.listdir(path) if LICENSE_FILE.match(f) and os.path.isfile(os.path.join(path, f)))


def bundle(rows, out):
    seen, parts = {}, []
    for r in rows:
        files = licence_files(r["path"])
        if r["name"] == "epaint_default_fonts":  # the fonts' own licences are in fonts/
            files += ["fonts/" + f for f in sorted(os.listdir(os.path.join(r["path"], "fonts"))) if f.endswith(".txt") and "license" in f.lower() or f in ("OFL.txt", "UFL.txt")]
        r["files"] = files
        r["upstream"] = []
        if not files and r["name"] in UPSTREAM:
            r["upstream"] = [os.path.join(UPSTREAM_DIR, f) for f in UPSTREAM[r["name"]][1]]
    with open(out, "w", encoding="utf-8") as o:
        o.write("OmaTree (egui) third-party licence bundle\n==========================================\n\n"
                "Generated by packaging/egui/third-party.py from the resolved Cargo.lock graph.\n"
                "Part 1 lists every compiled crate with its declared licence expression. Part 2 gives the licence\n"
                "and notice files found in each crate's own source, each distinct text once, with the crates it\n"
                "came from. A crate with no licence file is named in part 1 as such: its declared expression applies.\n"
                "Bundled SQLite (libsqlite3-sys) is public domain and needs no text.\n\n"
                "PART 1: crates\n--------------\n\n")
        for r in rows:
            note = "" if r["files"] else ("  [no licence file in the crate source; upstream copy in part 2]" if r["upstream"] else "  [no licence file in the crate source]")
            o.write(f"{r['name']} {r['version']}: {r['license']}{note}\n")
        o.write("\n\nPART 2: licence and notice texts\n--------------------------------\n")
        for r in rows:
            for f in r["files"]:
                data = open(os.path.join(r["path"], f), "rb").read()
                h = hashlib.sha256(data.strip()).hexdigest()
                seen.setdefault(h, dict(data=data, name=os.path.basename(f), who=[]))["who"].append(f"{r['name']} {r['version']}")
        for r in rows:
            for f in r["upstream"]:
                data = open(f, "rb").read()
                h = hashlib.sha256(data.strip()).hexdigest()
                e = seen.setdefault(h, dict(data=data, name=os.path.basename(f), who=[]))
                e["who"].append(f"{r['name']} {r['version']} (upstream copy, see packaging/egui/licences/SOURCES.md)")
        for h, e in sorted(seen.items(), key=lambda kv: (kv[1]["name"], kv[0])):
            o.write(f"\n\n{'=' * 78}\n{e['name']}: used by {len(e['who'])} crate(s): {', '.join(e['who'])}\n{'=' * 78}\n")
            o.write(e["data"].decode("utf-8", "replace").rstrip() + "\n")
    return len(rows), len(seen), [r["name"] for r in rows if not r["files"] and not r["upstream"]]


def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    rows = graph()
    if cmd == "table":
        print(table(rows))
    elif cmd in ("check", "update"):
        f = sys.argv[2]
        s = open(f, encoding="utf-8").read()
        m = re.search(re.escape(BEGIN) + ".*?" + re.escape(END), s, re.S)
        if not m:
            sys.exit(f"{f}: no generated block ({BEGIN})")
        new = s[:m.start()] + block(rows) + s[m.end():]
        if cmd == "update":
            open(f, "w", encoding="utf-8").write(new)
            print(f"{f}: {len(rows)} crates")
        elif new != s:
            sys.exit(f"{f}: generated crate table is out of date; run: packaging/egui/third-party.py update {f}")
        else:
            print(f"{f}: crate table is current ({len(rows)} crates)")
    elif cmd == "bundle":
        n, texts, nofile = bundle(rows, sys.argv[2])
        print(f"{sys.argv[2]}: {n} crates, {texts} distinct texts, {len(nofile)} without a licence file")
        missing = [r["name"] for r in rows if r["license"] == "MISSING"]
        if missing:
            print("MISSING licence metadata (manual review):", ", ".join(missing))
    else:
        sys.exit(__doc__)


main()
