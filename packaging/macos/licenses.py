#!/usr/bin/env python3
"""Writes the third-party notices of a finished OmaTree.app, from what is in it.

    packaging/macos/licenses.py OmaTree.app

Every library and Qt framework in Contents/Frameworks is traced back to the
Homebrew formula it was copied from (the bundle is built from Homebrew's Qt and
its dependencies), and the formula's name, version, licence, home page and source
archive are written to Contents/Resources/licenses/THIRD_PARTY_NOTICES.md, with
the licence files that come with each formula under third-party/. Nothing is
listed that is not in the bundle, and the script fails if a file in the bundle
cannot be traced, so the notices cannot silently fall behind the bundle.
(verify-app.sh checks the result again.)
"""
import glob
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))

# Files of a formula's top-level directory that are licence texts.
LICENCE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|LGPL|GPL)", re.I)


def die(message):
    sys.exit("licenses.py: " + message)


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def main():
    if len(sys.argv) != 2:
        die("usage: licenses.py OmaTree.app")
    app = os.path.abspath(sys.argv[1])
    frameworks = os.path.join(app, "Contents", "Frameworks")
    if not os.path.isdir(frameworks):
        die("no Contents/Frameworks in " + app)
    try:
        cellar = run("brew", "--cellar").strip()
    except (OSError, subprocess.CalledProcessError):
        die("Homebrew is needed to look up the libraries' licences (brew not found)")

    # 1. Which keg did each bundled file come from?
    owners = {}  # (formula, keg directory) -> [bundled names]
    for name in sorted(os.listdir(frameworks)):
        pattern = os.path.join(cellar, "*", "*", "lib", name)
        # Resolve symlinks: Homebrew's `qt` formula only links to qtbase,
        # qtdeclarative and the rest, and the real keg is what has the licence.
        real = sorted({os.path.realpath(h) for h in glob.glob(pattern)})
        if not real:
            die("cannot trace %s to a Homebrew formula (looked for %s)" % (name, pattern))
        if len(real) > 1:
            die("%s is in more than one keg: %s" % (name, ", ".join(real)))
        keg = os.path.dirname(os.path.dirname(real[0]))
        formula = os.path.relpath(keg, os.path.realpath(cellar)).split(os.sep)[0]
        owners.setdefault((formula, keg), []).append(name)

    # 2. What does Homebrew know about those formulae?
    names = sorted({f for f, _ in owners})
    info = {f["name"]: f for f in json.loads(run("brew", "info", "--json=v2", "--formula", *names))["formulae"]}

    out = os.path.join(app, "Contents", "Resources", "licenses")
    texts = os.path.join(out, "third-party")
    shutil.rmtree(texts, ignore_errors=True)
    os.makedirs(texts)

    # 3. The two texts the Qt LGPL refers to.
    for src, dst in (("LGPL-3.0.txt", "Qt-LGPL-3.0.txt"), ("GPL-3.0.txt", "Qt-GPL-3.0.txt")):
        shutil.copyfile(os.path.join(HERE, "licenses", src), os.path.join(out, dst))

    qt_names = sorted(f for (formula, _), files in owners.items() for f in files if f.startswith("Qt"))
    if "qtbase" not in info or "qtdeclarative" not in info:
        die("Qt was not traced to qtbase and qtdeclarative")
    qt_version = info["qtbase"]["versions"]["stable"]
    if info["qtdeclarative"]["versions"]["stable"] != qt_version:
        die("qtbase and qtdeclarative are different versions")
    rows = []
    for (formula, keg), files in sorted(owners.items()):
        meta = info[formula]
        version = meta["versions"]["stable"]
        base = "%s-%s" % (formula.replace("@", "-"), version)
        found = sorted(f for f in os.listdir(keg) if LICENCE_FILE.match(f) and os.path.isfile(os.path.join(keg, f)))
        for f in found:
            os.makedirs(os.path.join(texts, base), exist_ok=True)
            shutil.copyfile(os.path.join(keg, f), os.path.join(texts, base, f))
        is_qt = formula.startswith("qt")
        if not found and not is_qt:
            die("no licence file found for %s (%s)" % (formula, keg))
        rows.append(
            "### %s %s\n\n"
            "- **Licence (as declared by its Homebrew formula):** %s\n"
            "- **Home page:** <%s>\n"
            "- **Source:** <%s>\n"
            "- **In this app:** %s\n"
            "- **Licence text:** %s\n"
            % (
                formula,
                version,
                meta["license"] or "not declared",
                meta["homepage"],
                meta["urls"]["stable"]["url"],
                ", ".join("`%s`" % f for f in files),
                ("`third-party/%s/` (%s)" % (base, ", ".join(found)))
                if found
                else "`Qt-LGPL-3.0.txt` and `Qt-GPL-3.0.txt` (see above)",
            )
        )

    notice = """# Third-party notices for OmaTree.app

OmaTree is licensed under MIT or Apache-2.0 (see `LICENSE-MIT`, `LICENSE-APACHE`).
This application **contains** the third-party software listed below, in
`Contents/Frameworks` (and, for Qt, in `Contents/PlugIns` and
`Contents/Resources/qml`). This list is made from the finished bundle by
`packaging/macos/licenses.py`; it is not legal advice. Apple's own system
frameworks and libraries are used from macOS and are not part of this bundle.

## Qt

OmaTree uses **Qt %(qt)s** (<https://www.qt.io/>), as built and packaged by
Homebrew from the official, unmodified source release (see the Source link of
`qtbase` and `qtdeclarative` below; any patches Homebrew applies are in its
formulae). Qt is copyright The Qt Company Ltd. and other contributors; Qt and
the Qt logo are trademarks of The Qt Company Ltd.

Qt modules in this bundle (`Contents/Frameworks`): %(modules)s. Its plug-ins
(the Cocoa platform plug-in and the TLS backends, in `Contents/PlugIns`) and
the QML modules QtQuick, QtQuick.Controls (Basic style only), Templates,
Layouts, Dialogs, Window, QtQml (Models, WorkerScript) and Qt.labs.folderlistmodel
(`Contents/Resources/qml`) are the same Qt release.

These Qt modules are used under the **GNU Lesser General Public License,
version 3** (`Qt-LGPL-3.0.txt`; the LGPL is an addition to the GNU General
Public License, version 3, in `Qt-GPL-3.0.txt`). Qt's own source also contains
third-party code under permissive licences (for example BSD-2-Clause and
BSD-3-Clause), documented in Qt's "Third-Party Licenses" pages and in the
`src/3rdparty` directories of the Qt sources; the licence expressions below
include them.

**You can replace Qt.** OmaTree is *dynamically linked* to Qt: the executable
`Contents/MacOS/omatree` loads the Qt frameworks from `Contents/Frameworks` at
run time, and nothing from Qt is compiled into it. To use your own build of a
compatible Qt 6 (the same minor version, 6.11), replace the `Qt*.framework`
directories in `Contents/Frameworks` (and plug-ins and QML modules if needed)
with yours, then sign the modified app again, for example
`codesign --force --deep --sign - OmaTree.app`. The complete corresponding
source of Qt %(qt)s is available from the Qt Project: the links under `qtbase`
and `qtdeclarative` below, and <https://download.qt.io/official_releases/qt/>.

## OmaTree's own dependencies

The Rust crates that are compiled into the executable (including SQLite, which
`rusqlite` builds in; public domain) are inventoried in `THIRD_PARTY.md`, in
this directory.

## Libraries bundled with Qt

All of these are dynamic libraries from Homebrew (bottles built by Homebrew from
the sources linked below), each copied unmodified, apart from install names and
run paths that were rewritten so they load from inside the app.

%(rows)s""" % {
        "qt": qt_version,
        "modules": ", ".join(sorted(n.replace(".framework", "") for n in qt_names)),
        "rows": "\n".join(rows),
    }
    with open(os.path.join(out, "THIRD_PARTY_NOTICES.md"), "w") as f:
        f.write(notice)
    print("licenses.py: %d libraries and frameworks from %d Homebrew formulae" % (sum(len(v) for v in owners.values()), len(owners)))


main()
