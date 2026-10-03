"""Minimal reader for decrypted Rocket League (UE3, version 868) packages.

Reads the name/import/export tables and an object's tagged properties: enough to pull numeric
settings (camera, particle systems) out of cooked packages. Arrays carry no element type on disk,
so elements are decoded heuristically (tagged structs, floats, ints).
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from pathlib import Path

# Native structs serialized as raw bytes instead of tagged properties.
IMMUTABLE_STRUCTS = {
    "Vector": "<3f",
    "Vector2D": "<2f",
    "Vector4": "<4f",
    "Rotator": "<3i",
    "Color": "<4B",  # B, G, R, A
    "LinearColor": "<4f",
    "Guid": "<4I",
    "Quat": "<4f",
    "Plane": "<4f",
    "IntPoint": "<2i",
    "Box": "<6fB",
}


@dataclass
class Export:
    index: int  # 1-based (object references use +index)
    class_index: int
    outer_index: int
    name: str
    archetype: int
    flags: int
    size: int
    offset: int


@dataclass
class Import:
    class_name: str
    outer_index: int
    name: str


@dataclass
class Package:
    path: Path
    data: bytes
    names: list[str] = field(default_factory=list)
    imports: list[Import] = field(default_factory=list)
    exports: list[Export] = field(default_factory=list)

    # ---- tables

    @classmethod
    def open(cls, path: Path) -> "Package":
        pkg = cls(Path(path), Path(path).read_bytes())
        pkg._read_tables()
        return pkg

    def _read_tables(self) -> None:
        d = self.data
        tag, ver, lic = struct.unpack_from("<IHH", d, 0)
        assert tag == 0x9E2A83C1, "not an Unreal package"
        pos = 12
        n = struct.unpack_from("<i", d, pos)[0]  # FolderName
        pos += 4 + (n if n >= 0 else -2 * n)
        pos += 4  # PackageFlags
        name_count, name_off, exp_count, exp_off, imp_count, imp_off = struct.unpack_from("<6i", d, pos)

        pos = name_off
        for _ in range(name_count):
            s, pos = self._fstring(pos)
            self.names.append(s)
            pos += 8  # flags

        pos = imp_off
        for _ in range(imp_count):
            _cls_pkg = self._fname(pos)
            cls_name = self._fname(pos + 8)
            outer = struct.unpack_from("<i", d, pos + 16)[0]
            name = self._fname(pos + 20)
            self.imports.append(Import(cls_name, outer, name))
            pos += 28

        pos = exp_off
        for i in range(exp_count):
            cls_i, _super, outer = struct.unpack_from("<3i", d, pos)
            name = self._fname(pos + 12)
            arch, flags, size, offset, _off_hi, _eflags, net_count = struct.unpack_from("<iQiiiIi", d, pos + 20)
            pos += 20 + 4 + 8 + 4 + 4 + 4 + 4 + 4 + 4 * net_count + 16 + 4
            self.exports.append(Export(i + 1, cls_i, outer, name, arch, flags, size, offset))

    def _fstring(self, pos: int) -> tuple[str, int]:
        n = struct.unpack_from("<i", self.data, pos)[0]
        pos += 4
        if n >= 0:
            s = self.data[pos : pos + n].rstrip(b"\0").decode("latin-1")
            return s, pos + n
        s = self.data[pos : pos - 2 * n].decode("utf-16-le").rstrip("\0")
        return s, pos - 2 * n

    def _fname(self, pos: int) -> str:
        i, num = struct.unpack_from("<ii", self.data, pos)
        if not 0 <= i < len(self.names):
            raise ValueError(f"bad name index {i}")
        return self.names[i] if num == 0 else f"{self.names[i]}_{num - 1}"

    # ---- object references

    def obj_name(self, ref: int) -> str | None:
        if ref == 0:
            return None
        if ref > 0:
            return self.exports[ref - 1].name
        return self.imports[-ref - 1].name

    def obj_path(self, ref: int) -> str | None:
        parts = []
        while ref:
            if ref > 0:
                e = self.exports[ref - 1]
                parts.append(e.name)
                ref = e.outer_index
            else:
                im = self.imports[-ref - 1]
                parts.append(im.name)
                ref = im.outer_index
        return ".".join(reversed(parts)) if parts else None

    def class_name(self, e: Export) -> str:
        if e.class_index == 0:
            return "Class"
        if e.class_index < 0:
            return self.imports[-e.class_index - 1].name
        return self.exports[e.class_index - 1].name

    def find(self, name: str, class_name: str | None = None) -> list[Export]:
        return [e for e in self.exports if e.name == name and (class_name is None or self.class_name(e) == class_name)]

    def full_path(self, e: Export) -> str:
        return self.obj_path(e.index) or e.name

    def export_at(self, path: str) -> Export | None:
        """The export with this full object path (`Package.Group.Name`)."""
        name = path.split(".")[-1]
        return next((e for e in self.exports if e.name == name and self.full_path(e) == path), None)

    def props_at(self, path: str | None) -> dict:
        """Tagged properties of the export at `path`, or {} if there is none."""
        e = self.export_at(path) if path else None
        return self.properties(e) if e else {}

    # ---- properties

    def properties(self, e: Export) -> dict:
        """Tagged properties of an export (`Default__X` objects, archetypes, particle modules...)."""
        end = e.offset + e.size
        pos = e.offset
        # UObject header: [component template info] + NetIndex. Find the first valid property tag.
        for skip in (4, 8, 12, 16, 0, 20, 24):
            try:
                props, _ = self._tagged(e.offset + skip, end)
                return props
            except (ValueError, struct.error, IndexError, UnicodeDecodeError):
                continue
        raise ValueError(f"could not parse properties of {e.name}")

    def _tagged(self, pos: int, end: int) -> tuple[dict, int]:
        props: dict = {}
        while True:
            if pos + 8 > end:
                raise ValueError("ran past object")
            name = self._fname(pos)
            pos += 8
            if name == "None":
                return props, pos
            ptype = self._fname(pos)
            size, aidx = struct.unpack_from("<ii", self.data, pos + 8)
            pos += 16
            if size < 0 or pos + size > end + 1:
                raise ValueError("bad size")
            extra = None
            if ptype == "StructProperty":
                extra = self._fname(pos)
                pos += 8
            elif ptype == "BoolProperty":
                value = self.data[pos] != 0
                pos += 1
                self._put(props, name, aidx, value)
                continue
            elif ptype == "ByteProperty":
                extra = self._fname(pos)
                pos += 8
            elif not ptype.endswith("Property"):
                raise ValueError(f"bad type {ptype}")
            value = self._value(ptype, extra, pos, size)
            pos += size
            self._put(props, name, aidx, value)

    @staticmethod
    def _put(props: dict, name: str, aidx: int, value) -> None:
        if aidx == 0 and name not in props:
            props[name] = value
        else:
            cur = props.get(name)
            if not isinstance(cur, dict) or "__static_array__" not in cur:
                cur = {"__static_array__": True, 0: cur} if name in props else {"__static_array__": True}
            cur[aidx] = value
            props[name] = cur

    def _value(self, ptype: str, extra, pos: int, size: int):
        d = self.data
        if ptype == "IntProperty":
            return struct.unpack_from("<i", d, pos)[0]
        if ptype == "FloatProperty":
            return struct.unpack_from("<f", d, pos)[0]
        if ptype in ("ObjectProperty", "ClassProperty", "ComponentProperty", "InterfaceProperty"):
            return {"__ref__": self.obj_path(struct.unpack_from("<i", d, pos)[0])}
        if ptype == "NameProperty":
            return self._fname(pos)
        if ptype == "StrProperty":
            return self._fstring(pos)[0]
        if ptype == "ByteProperty":
            return self._fname(pos) if size == 8 else d[pos]
        if ptype == "StructProperty":
            fmt = IMMUTABLE_STRUCTS.get(extra)
            if fmt and struct.calcsize(fmt) == size:
                return {"__struct__": extra, "v": list(struct.unpack_from(fmt, d, pos))}
            inner, _ = self._tagged(pos, pos + size)
            inner["__struct__"] = extra
            return inner
        if ptype == "ArrayProperty":
            return self._array(pos, size)
        if ptype == "DelegateProperty":
            return None
        return d[pos : pos + size].hex()

    def _array(self, pos: int, size: int):
        d = self.data
        count = struct.unpack_from("<i", d, pos)[0]
        body = pos + 4
        n = size - 4
        if count == 0:
            return []
        # Array of tagged structs?
        try:
            out, p = [], body
            for _ in range(count):
                item, p = self._tagged(p, pos + size)
                out.append(item)
            if p == pos + size:
                return out
        except (ValueError, struct.error, IndexError, UnicodeDecodeError):
            pass
        if n == 4 * count:
            ints = struct.unpack_from(f"<{count}i", d, body)
            floats = struct.unpack_from(f"<{count}f", d, body)
            # Small integers are most likely object refs / ints; otherwise floats.
            if all(-len(self.imports) <= v <= len(self.exports) for v in ints) and any(v != 0 for v in ints) and not all(abs(f) > 1e-30 for f in floats):
                return [{"__ref__": self.obj_path(v)} for v in ints]
            return list(floats)
        if n == 8 * count:
            try:
                return [self._fname(body + 8 * i) for i in range(count)]
            except ValueError:
                pass
        if n == 12 * count:
            return [list(struct.unpack_from("<3f", d, body + 12 * i)) for i in range(count)]
        if n == 16 * count:
            return [list(struct.unpack_from("<4f", d, body + 16 * i)) for i in range(count)]
        return {"__raw__": d[body : pos + size].hex(), "count": count}
