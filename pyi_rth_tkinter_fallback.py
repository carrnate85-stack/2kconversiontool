import os
import sys


runtime_root = getattr(sys, "_MEIPASS", os.path.dirname(sys.executable))
os.environ["TCL_LIBRARY"] = os.path.join(runtime_root, "_tcl_data")
os.environ["TK_LIBRARY"] = os.path.join(runtime_root, "_tk_data")
