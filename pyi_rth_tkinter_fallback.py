import ctypes
import os
import sys


runtime_root = getattr(sys, "_MEIPASS", os.path.dirname(sys.executable))
if sys.platform == "win32":
    tcl_dll_path = os.path.join(runtime_root, "tcl86t.dll")
    if os.path.isfile(tcl_dll_path):
        tcl = ctypes.CDLL(tcl_dll_path)
        tcl.Tcl_FindExecutable.argtypes = [ctypes.c_char_p]
        tcl.Tcl_FindExecutable(os.fsencode(sys.executable.replace("\\", "/")))
os.environ["TCL_LIBRARY"] = os.path.join(runtime_root, "_tcl_data")
os.environ["TK_LIBRARY"] = os.path.join(runtime_root, "_tk_data")
