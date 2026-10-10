"""Verify the built Windows executable embeds every supplied ICO image."""
import ctypes
from ctypes import wintypes
from pathlib import Path
import struct
import sys

from PIL import Image

root = Path(__file__).resolve().parent.parent
exe = Path(sys.argv[1]) if len(sys.argv) > 1 else root / "target/release/wacu.exe"
kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
kernel.LoadLibraryExW.restype = wintypes.HMODULE
kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
kernel.FindResourceW.restype = wintypes.HANDLE
kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
kernel.SizeofResource.restype = wintypes.DWORD
kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
kernel.LoadResource.restype = wintypes.HANDLE
kernel.LockResource.argtypes = [wintypes.HANDLE]
kernel.LockResource.restype = ctypes.c_void_p
kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
kernel.FreeLibrary.restype = wintypes.BOOL
module = kernel.LoadLibraryExW(str(exe.resolve()), None, 2)
if not module:
    raise ctypes.WinError(ctypes.get_last_error())


def resource(identifier, kind):
    handle = kernel.FindResourceW(module, identifier, kind)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    size = kernel.SizeofResource(module, handle)
    address = kernel.LockResource(kernel.LoadResource(module, handle))
    if not address or not size:
        raise ctypes.WinError(ctypes.get_last_error())
    return ctypes.string_at(address, size)


try:
    source = (root / "assets/app.ico").read_bytes()
    expected = {}
    for index in range(struct.unpack_from("<H", source, 4)[0]):
        w, h, _, _, _, _, size, offset = struct.unpack_from("<BBBBHHII", source, 6 + index * 16)
        expected[(w or 256, h or 256)] = source[offset : offset + size]
    group = resource(1, 14)
    _, kind, count = struct.unpack_from("<HHH", group)
    assert kind == 1 and count == len(expected), "Missing icon resolutions"
    checked = []
    for index in range(count):
        w, h, _, _, _, _, size, identifier = struct.unpack_from("<BBBBHHIH", group, 6 + index * 14)
        dimensions = (w or 256, h or 256)
        payload = resource(identifier, 3)
        assert len(payload) == size and payload == expected[dimensions], dimensions
        checked.append(dimensions[0])
    print("EXE brand icon verified:", sorted(checked))
    Image.open(root / "assets/app.ico").convert("RGBA").save(root / "target/exe-icon.png")
finally:
    kernel.FreeLibrary(module)
