# Unpack

标记性状。实现时，元素可以像记录一样通过模式匹配来分解

```erg
C = Class {i = Int}, Impl=Unpack
C.new i = Self::new {i;}
{i} = C.new(1)
D = Class C or Int
log match D.new(1):
    (i: Int) -> i
    ({i}: C) -> i
```