#!/bin/bash
# Checks a built OmaTree.app: identity, icon, document type, architecture,
# that every dependency is a system library or inside the bundle, that no
# build-machine path is left, that the signature is intact, and that the
# application starts with an empty environment.
#
#   packaging/macos/verify-app.sh path/to/OmaTree.app
set -euo pipefail

app=${1:?usage: verify-app.sh OmaTree.app}
app=$(cd "$app" && pwd)
c=$app/Contents
app_real=$(cd "$app" && pwd -P)
fail=0
bad() { echo "FAIL: $*" >&2; fail=1; }
ok() { echo "ok:   $*"; }

# ---- identity ---------------------------------------------------------------
plist=$c/Info.plist
pb() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || echo "<missing>"; }
plutil -lint "$plist" >/dev/null || bad "Info.plist is not valid"
[ "$(pb CFBundleIdentifier)" = io.github.Dorotabro.OmaTree ] && ok "bundle identifier" || bad "bundle identifier: $(pb CFBundleIdentifier)"
[ "$(pb CFBundleName)" = OmaTree ] && [ "$(pb CFBundleDisplayName)" = OmaTree ] && ok "name OmaTree" || bad "bundle name"
[ "$(pb CFBundleExecutable)" = omatree ] && [ -x "$c/MacOS/omatree" ] && ok "executable omatree" || bad "executable"
[ "$(pb CFBundlePackageType)" = APPL ] && ok "package type APPL" || bad "package type"
[ -f "$c/Resources/$(pb CFBundleIconFile).icns" ] && ok "icon $(pb CFBundleIconFile).icns" || bad "icon file missing"
[ "$(pb 'CFBundleDocumentTypes:0:LSItemContentTypes:0')" = io.github.Dorotabro.OmaTree.notebook ] \
    && [ "$(pb 'UTExportedTypeDeclarations:0:UTTypeTagSpecification:public.filename-extension:0')" = omatree ] \
    && ok "document type .omatree" || bad "document type"
[ "$(pb 'UTExportedTypeDeclarations:0:UTTypeConformsTo:0')" = public.data ] && ok "notebook type conforms to public.data only" || bad "UTI conformance"

# ---- every Mach-O -----------------------------------------------------------
n=0
while IFS= read -r f; do
    file -b "$f" | grep -q 'Mach-O' || continue
    n=$((n + 1))
    [ "$(lipo -archs "$f")" = arm64 ] || bad "not arm64-only: ${f#"$c"/}"
    # run paths: only relative ones, and only ones that lead inside the bundle
    rpaths=()
    while IFS= read -r rp; do
        case "$rp" in
            @loader_path*) r=$(dirname "$f")/${rp#@loader_path} ;;
            @executable_path*) r=$c/MacOS/${rp#@executable_path} ;;
            *) bad "absolute rpath $rp in ${f#"$c"/}"; continue ;;
        esac
        r=$(cd "$r" 2>/dev/null && pwd -P) || { bad "rpath $rp of ${f#"$c"/} does not exist"; continue; }
        case "$r" in "$app_real"/*|"$app_real") rpaths+=("$r") ;; *) bad "rpath $rp of ${f#"$c"/} leaves the bundle ($r)" ;; esac
    done < <(otool -l "$f" | awk '/cmd LC_RPATH/{getline; getline; print $2}')
    case "$f" in
        *.dylib) id=$(otool -D "$f" | tail -n +2)
            # (a plug-in has none, and is found by path)
            case "$id" in ""|@rpath/*|@loader_path/*|@executable_path/*) ;; *) bad "install name $id of ${f#"$c"/}" ;; esac ;;
    esac
    # (for a library the first line is its own install name, checked above)
    deps=$(otool -L "$f" | tail -n +2 | awk '{print $1}')
    case "$f" in *.dylib) deps=$(echo "$deps" | tail -n +2) ;; esac
    while IFS= read -r dep; do
        [ -n "$dep" ] || continue
        case "$dep" in
            /usr/lib/*|/System/Library/*) ;;
            @executable_path/*)
                [ -e "$c/MacOS/${dep#@executable_path/}" ] || bad "unresolved $dep in ${f#"$c"/}" ;;
            @loader_path/*)
                [ -e "$(dirname "$f")/${dep#@loader_path/}" ] || bad "unresolved $dep in ${f#"$c"/}" ;;
            @rpath/*)
                found=0
                for r in ${rpaths[@]+"${rpaths[@]}"}; do [ -e "$r/${dep#@rpath/}" ] && found=1; done
                [ "$found" = 1 ] || bad "$dep of ${f#"$c"/} cannot be found through its run paths" ;;
            *) bad "outside dependency $dep in ${f#"$c"/}" ;;
        esac
    done <<< "$deps"
done < <(find "$c" -type f \( -perm -u+x -o -name '*.dylib' \))
ok "$n Mach-O files checked: arm64 only, relative rpaths, system or bundled dependencies"

# ---- build-machine paths in what is ours -------------------------------------
if strings -a "$c/MacOS/omatree" | grep -E "/Users/|/opt/homebrew|$HOME" | grep -q .; then
    bad "build paths in the executable:"; strings -a "$c/MacOS/omatree" | grep -E "/Users/|/opt/homebrew|$HOME" | head -5 >&2
else
    ok "no build paths in the executable"
fi
if grep -rIl --include='*.qml' --include='qmldir' --include='*.conf' --include='*.plist' -E '/Users/|/opt/homebrew' "$c" 2>/dev/null | grep -q .; then
    bad "build paths in QML or configuration"
else
    ok "no build paths in QML, qmldir or configuration"
fi

# ---- signature ---------------------------------------------------------------
if err=$(codesign --verify --deep --strict "$app" 2>&1); then ok "codesign --verify --deep --strict"; else bad "codesign: $err"; fi

# ---- it starts with nothing but an empty environment ---------------------------
log=$(mktemp)
env -i "$c/MacOS/omatree" >"$log" 2>&1 &
pid=$!
sleep 4
if kill -0 "$pid" 2>/dev/null; then
    ok "starts with an empty environment and keeps running"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
else
    bad "the application did not stay up"
fi
if [ -s "$log" ]; then bad "unexpected output on start:"; head -5 "$log" >&2; else ok "no output on start"; fi
rm -f "$log"

[ "$fail" = 0 ] || { echo "verify-app.sh: FAILED" >&2; exit 1; }
echo "verify-app.sh: all checks passed"
