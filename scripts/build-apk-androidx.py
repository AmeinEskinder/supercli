#!/usr/bin/env python3
"""Resolve the androidx AAR/JAR closure for the manual APK build.

BFS over POM dependencies from Google's Maven repo starting at the seeds
below, downloads each artifact once into --out, extracts classes.jar from
AARs as <artifact>-<version>-classes.jar, and writes MANIFEST.txt.

Idempotent: existing files are reused, so reruns are cheap.
Requires network access to dl.google.com.
"""
import argparse, os, sys, urllib.request, xml.etree.ElementTree as ET, zipfile

BASE = "https://dl.google.com/dl/android/maven2"

SEEDS = [
    ("androidx.appcompat", "appcompat", "1.7.0"),
    ("androidx.webkit", "webkit", "1.10.0"),
]

def gpath(g, a, v, ext):
    return f"{BASE}/{g.replace('.', '/')}/{a}/{v}/{a}-{v}.{ext}"

def norm_version(v):
    # Maven version ranges like [2.6.1] or [2.6.1,2.7.0): resolve to the
    # lower bound so the artifact URL is concrete.
    v = v.strip()
    if v[:1] in "[(" and v[-1:] in "])":
        v = v[1:-1].split(",")[0].strip()
    return v

def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, help="directory for AAR/JAR artifacts")
    args = ap.parse_args()
    out = args.out
    os.makedirs(out, exist_ok=True)

    resolved, queue, order = {}, list(SEEDS), []
    while queue:
        g, a, v = queue.pop(0)
        if (g, a) in resolved:
            continue
        try:
            data = fetch(gpath(g, a, v, "pom"))
        except Exception as e:
            print(f"POM FAIL {g}:{a}:{v}: {e}", file=sys.stderr)
            continue
        root = ET.fromstring(data)
        ns = {"m": "http://maven.apache.org/POM/4.0.0"}
        packaging = root.findtext("m:packaging", namespaces=ns) or "jar"
        resolved[(g, a)] = (v, packaging)
        order.append((g, a, v, packaging))
        deps = root.find("m:dependencies", ns)
        if deps is None:
            continue
        for d in deps.findall("m:dependency", ns):
            dg = d.findtext("m:groupId", namespaces=ns)
            da = d.findtext("m:artifactId", namespaces=ns)
            dv = d.findtext("m:version", namespaces=ns)
            scope = d.findtext("m:scope", namespaces=ns) or "compile"
            if not (dg and da and dv):
                continue
            if scope in ("test", "provided"):
                continue
            if (dg, da) not in resolved and not any(x[0] == dg and x[1] == da for x in queue):
                queue.append((dg, da, norm_version(dv)))

    print(f"resolved {len(order)} artifacts")
    manifest = []
    for g, a, v, typ in order:
        ext = "aar" if typ == "aar" else "jar"
        dest = os.path.join(out, f"{a}-{v}.{ext}")
        if not os.path.exists(dest):
            try:
                data = fetch(gpath(g, a, v, ext))
            except Exception as e:
                print(f"FAIL {a}-{v}.{ext}: {e}", file=sys.stderr)
                continue
            open(dest, "wb").write(data)
            print(f"got {a}-{v}.{ext} ({len(data)//1024}k)")
        if ext == "aar":
            cj = os.path.join(out, f"{a}-{v}-classes.jar")
            if not os.path.exists(cj):
                try:
                    with zipfile.ZipFile(dest) as z:
                        with z.open("classes.jar") as zi, open(cj, "wb") as fo:
                            fo.write(zi.read())
                    print(f"  extracted {a}-{v}-classes.jar")
                except KeyError:
                    print(f"  NOTE: no classes.jar in {a}-{v}.aar", file=sys.stderr)
        manifest.append(f"{g}:{a}:{v}:{ext}")

    open(os.path.join(out, "MANIFEST.txt"), "w").write("\n".join(manifest) + "\n")
    print(f"MANIFEST written ({len(manifest)} entries) -> {out}")

if __name__ == "__main__":
    main()
