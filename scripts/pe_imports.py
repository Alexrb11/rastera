"""Inspect direct and delayed imports of a PE32+ binary without loading it."""
import struct

WINDOWS_DLLS = {
    'advapi32.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'combase.dll',
    'crypt32.dll', 'd3d12.dll', 'd3dcompiler_47.dll', 'dbghelp.dll',
    'dwmapi.dll', 'dxgi.dll', 'gdi32.dll', 'hid.dll', 'imm32.dll',
    'kernel32.dll', 'msvcrt.dll', 'normaliz.dll', 'ntdll.dll',
    'ole32.dll', 'oleaut32.dll', 'opengl32.dll', 'psapi.dll',
    'secur32.dll', 'setupapi.dll', 'shcore.dll', 'shell32.dll',
    'uiautomationcore.dll', 'user32.dll', 'ucrtbase.dll', 'uxtheme.dll',
    'version.dll', 'winmm.dll', 'wintrust.dll', 'ws2_32.dll',
}


def imports(data):
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    optional = pe + 24
    if data[:2] != b'MZ' or data[pe:pe + 4] != b'PE\0\0' or struct.unpack_from('<H', data, pe + 4)[0] != 0x8664 or struct.unpack_from('<H', data, optional)[0] != 0x20b:
        raise ValueError('Expected a PE32+ Windows x64 binary')
    if struct.unpack_from('<H', data, optional + 68)[0] != 2:
        raise ValueError('Expected a graphical Windows binary')
    count = struct.unpack_from('<H', data, pe + 6)[0]
    section_table = optional + struct.unpack_from('<H', data, pe + 20)[0]
    sections = [struct.unpack_from('<IIII', data, section_table + i * 40 + 8) for i in range(count)]
    image_base = struct.unpack_from('<Q', data, optional + 24)[0]

    def offset(rva):
        for virtual_size, virtual_address, raw_size, raw_offset in sections:
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                result = raw_offset + rva - virtual_address
                if result >= len(data):
                    break
                return result
        raise ValueError('Invalid import RVA')

    result = set()
    for directory, stride in [(1, 20), (13, 32)]:
        rva, size = struct.unpack_from('<II', data, optional + 112 + directory * 8)
        if not rva:
            continue
        start = offset(rva)
        for position in range(start, start + size, stride):
            entry = data[position:position + stride]
            if not any(entry):
                break
            if len(entry) != stride:
                raise ValueError('Truncated import descriptor')
            if directory == 1:
                name = struct.unpack_from('<I', entry, 12)[0]
            else:
                attributes, name = struct.unpack_from('<II', entry)
                if not attributes & 1:
                    name -= image_base
            name_offset = offset(name)
            end = data.find(b'\0', name_offset, name_offset + 260)
            if end < 0:
                raise ValueError('Invalid imported DLL name')
            result.add(data[name_offset:end].decode('ascii').lower())
    if not result:
        raise ValueError('Expected Windows system imports')
    foreign = {name for name in result if name not in WINDOWS_DLLS and not name.startswith(('api-ms-win-', 'ext-ms-win-'))}
    if foreign:
        raise ValueError('Non-system DLLs must be resolved before portable distribution: ' + ', '.join(sorted(foreign)))
    return sorted(result)
