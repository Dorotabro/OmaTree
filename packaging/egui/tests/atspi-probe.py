"""Prints the accessibility tree of a running omatree-egui (and the text of its entries). Run by
atspi-probe.sh on a private session bus; needs python3 with PyGObject and the Atspi typelib."""
import gi, sys, time
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi
def walk(n, d=0, out=None):
    out = [] if out is None else out
    try:
        role = n.get_role_name(); name = n.get_name()
    except Exception as e:
        return out
    txt = None
    try:
        ti = n.get_text_iface()
        if ti: 
            try: txt = Atspi.Text.get_text(ti, 0, min(Atspi.Text.get_character_count(ti), 60))
            except Exception as e: txt = "TEXT-READ-FAILED: %s" % str(e)[:90]
    except Exception: pass
    out.append("%s%s %r%s" % ("  "*d, role, name, "" if txt is None else "  text=%r" % txt))
    for i in range(n.get_child_count()):
        try: walk(n.get_child_at_index(i), d+1, out)
        except Exception: pass
    return out
Atspi.init()
desk = Atspi.get_desktop(0)
for i in range(desk.get_child_count()):
    app = desk.get_child_at_index(i)
    if app.get_name() and "omatree" in app.get_name().lower() or app.get_name()=="OmaTree":
        print("\n".join(walk(app)))

# The note title read back in every way an assistive technology might ask (code points, not UTF-16).
def find(n, name):
    if n.get_name() == name: return n
    for i in range(n.get_child_count()):
        r = find(n.get_child_at_index(i), name)
        if r: return r
for i in range(desk.get_child_count()):
    a = desk.get_child_at_index(i)
    if a.get_name() == "omatree-egui":
        e = find(a, "Note title")
        if e and e.get_text_iface():
            t = e.get_text_iface(); n = Atspi.Text.get_character_count(t)
            print("title: character_count", n)
            for args in ((0, n), (0, -1), (n - 1, n)):
                try: print("title: get_text%s = %r" % (args, Atspi.Text.get_text(t, *args)))
                except Exception as ex: print("title: get_text%s FAILED %s" % (args, str(ex)[:100]))
