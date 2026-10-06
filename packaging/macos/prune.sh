# Sourced by build-dmg.sh. Removes from a macdeployqt'd bundle everything
# OmaTree does not use.
#
# macdeployqt, given the application's QML, copies the whole of every module
# it imports: all six Quick Controls styles, the virtual keyboard, shapes,
# particles, PDF, multimedia, SQL, every image format, and the libraries those
# need. OmaTree uses the Basic style only (main.rs sets it), draws its own UI,
# opens no network connection and loads no images. The lists below are what Qt
# really loads, measured with DYLD_PRINT_LIBRARIES over the whole integration
# suite on the real macOS platform plugin, plus the QML modules main.qml and
# its dialogs import. docs/PACKAGING.md has the details.

# QML module directories (below Resources/qml) that stay. Anything else goes.
KEEP_QML='QML Qt/labs/folderlistmodel QtQml QtQml/Models QtQml/WorkerScript QtQuick QtQuick/Controls QtQuick/Controls/Basic QtQuick/Controls/impl QtQuick/Dialogs QtQuick/Dialogs/quickimpl QtQuick/Layouts QtQuick/Templates QtQuick/Window'

# QML plugin libraries (macdeployqt puts them in PlugIns/quick) that stay.
KEEP_QML_PLUGINS='libmodelsplugin libqmlplugin libqmlfolderlistmodelplugin libqquicklayoutsplugin libqtquick2plugin libqtquickcontrols2basicstyleimplplugin libqtquickcontrols2basicstyleplugin libqtquickcontrols2implplugin libqtquickcontrols2plugin libqtquickdialogs2quickimplplugin libqtquickdialogsplugin libqtquicktemplates2plugin libquickwindowplugin libworkerscriptplugin'

# Qt plugins that stay, as directory/file. The Cocoa platform plugin, and the
# TLS backends QtNetwork loads when it starts: the system one (Secure
# Transport) and the certificates-only one. The OpenSSL backend is not needed.
KEEP_PLUGINS='platforms/libqcocoa.dylib tls/libqsecuretransportbackend.dylib tls/libqcertonlybackend.dylib'

contains() { case " $1 " in *" $2 "*) return 0 ;; esac; return 1; }

# Prints the key under which a Mach-O dependency is found in Frameworks/:
# "QtCore.framework" or "libpng16.16.dylib".
dep_key() {
    case "$1" in
        *.framework/*) local k=${1%%.framework/*}; echo "${k##*/}.framework" ;;
        *) echo "${1##*/}" ;;
    esac
}

is_macho() { file -b "$1" 2>/dev/null | grep -q 'Mach-O'; }

prune_qt() {
    local app=$1 res=$1/Contents/Resources/qml plug=$1/Contents/PlugIns fw=$1/Contents/Frameworks
    local d f rel name

    # QML modules: remove every directory not on the list, deepest first.
    while IFS= read -r d; do
        rel=${d#"$res"/}
        contains "$KEEP_QML" "$rel" && continue
        case " $KEEP_QML " in *" $rel/"*) continue ;; esac   # parent of a kept one
        rm -rf "$d"
    done < <(find "$res" -mindepth 1 -type d | awk '{print length($0) " " $0}' | sort -rn | cut -d' ' -f2-)

    # QML plugin libraries.
    for f in "$plug"/quick/*.dylib; do
        name=$(basename "$f" .dylib)
        contains "$KEEP_QML_PLUGINS" "$name" || rm -f "$f"
    done

    # Qt plugins.
    while IFS= read -r f; do
        rel=${f#"$plug"/}
        case "$rel" in quick/*) continue ;; esac
        contains "$KEEP_PLUGINS" "$rel" || rm -f "$f"
    done < <(find "$plug" -type f)
    find "$plug" -type d -empty -delete

    # Libraries nothing that is left refers to, until none is left over.
    local changed=1 refs line key self
    while [ "$changed" = 1 ]; do
        changed=0
        refs=$'\n'
        while IFS= read -r f; do
            is_macho "$f" || continue
            case "$f" in
                */Frameworks/*) self=$(dep_key "${f#*/Frameworks/}") ;;
                *) self= ;;
            esac
            while IFS= read -r line; do
                key=$(dep_key "$line")
                [ "$key" = "$self" ] && continue
                refs+="$key"$'\n'
            done < <(otool -L "$f" | tail -n +2 | awk '{print $1}')
        done < <(find "$app/Contents" -type f \( -name '*.dylib' -o -path '*/MacOS/*' -o -path '*/Frameworks/*.framework/Versions/*/*' \) ! -name '*.plist' ! -name '*.prl')
        for f in "$fw"/*; do
            key=$(basename "$f")
            case "$refs" in *$'\n'"$key"$'\n'*) ;; *) rm -rf "$f"; changed=1 ;; esac
        done
    done
}

# macdeployqt leaves the build machine's (Homebrew's) library search paths and
# some install names behind, several of them pointing outside the bundle or
# nowhere. Nothing in the bundle may. Every run path is deleted and the one
# that is right for where the file sits is added, and bundled libraries get
# @rpath install names.
make_relocatable() {
    local app=$1 f rp id want
    while IFS= read -r f; do
        is_macho "$f" || continue
        while IFS= read -r rp; do
            install_name_tool -delete_rpath "$rp" "$f"
        done < <(otool -l "$f" | awk '/cmd LC_RPATH/{getline; getline; print $2}')
        want=
        case "$f" in
            */MacOS/*) want=@executable_path/../Frameworks ;;
            */Frameworks/*.framework/*) want=@loader_path/../../../ ;;
            */Frameworks/*.dylib) want=@loader_path ;;
            */PlugIns/*/*.dylib) want=@loader_path/../../Frameworks ;;
        esac
        [ -n "$want" ] && install_name_tool -add_rpath "$want" "$f"
        case "$f" in
            */Frameworks/*.dylib)
                id=$(otool -D "$f" | tail -n +2)
                case "$id" in @rpath/*) ;; *) install_name_tool -id "@rpath/$(basename "$f")" "$f" ;; esac ;;
        esac
    done < <(find "$app/Contents" -type f \( -name '*.dylib' -o -path '*/MacOS/*' -o -path '*/Frameworks/*.framework/Versions/*/*' \) ! -name '*.plist' ! -name '*.prl')
}
