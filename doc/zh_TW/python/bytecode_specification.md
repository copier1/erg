# Python bytecode specification

## Format

* 0~3 byte(u32): magic number (see common/bytecode.rs for details)
* 4~7 byte(u32): 0 padding
* 8~12 byte(u32): timestamp
* 13~ byte(PyCodeObject): code object

## PyCodeObject

* 0     byte(u8): '0xe3' (prefix, this means code's 'c')
* 01~04 byte(u32): number of args (co_argcount)
* 05~08 byte(u32): number of position-only args (co_posonlyargcount)
* 09~12 byte(u32): number of keyword-only args (co_kwonlyargcount)
* 13~16 byte(u32): number of locals (co_nlocals)
* 17~20 byte(u32): stack size (co_stacksize)
* 21~24 byte(u32): flags (co_flags) ()
* ?     byte: bytecode instructions, ends with '0x53', '0x0' (83, 0): RETURN_VALUE (co_code)
* ?     byte(PyTuple): constants used in the code (co_consts)
* ?     byte(PyTuple): names used in the code (co_names)
* ?     byte(PyTuple): variable names defined in the code, include params (PyTuple) (co_varnames)
* ?     byte(PyTuple): variables captured from the outer scope (co_freevars)
* ?     byte(PyTuple): variables used in the inner closure (co_cellvars)
* ?     byte(PyUnicode or PyShortAscii): file name, where it was loaded from (co_filename)
* ?     byte(PyUnicode or PyShortAscii): the name of code itself, default is \<module\> (co_name)
* ?~?+3 byte(u32): number of first line (co_firstlineno)
* ?     byte(bytes): line table, represented by PyStringObject? (co_lnotab)

## PyTupleObject

* 0     byte: 0x29 (means ')')
* 01~04 byte(u32): number of tuple items
* ?     byte(PyObject): items

## PyStringObject

* 如果我使用ascii以外的字符，它會變成PyUnicode嗎？
* "あ", "𠮷“，”和“α”是PyUnicode（不再使用？）

* 0     byte: 0x73 (means 's')
* 1~4   byte: length of string
* 5~    byte: payload

## PyUnicodeObject

* 0     byte: 0x75 (means 'u')
* 1~4   byte: length of string
* 5~    byte: payload

## PyShortAsciiObject

* 說是 short，100 個以上的字也是這個
* 或者說不是 short 沒有 ascii（short 是數據類型？）

* 0     byte: 0xFA (means 'z')
* 1~4   byte: length of string
* 5~    byte: payload

## PyInternedObject

** intern 化的對象註冊在專用的 map 中，可以用 is 進行比較例如字符串等可以在不考慮長度的常量時間內進行比較

* 0     byte: 0x74 (means 't')

## PyShortAsciiInternedObject

* 0     byte: 0xDA (means 'Z')
* 1~4   byte: length of string
* 5~    byte: payload