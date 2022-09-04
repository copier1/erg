# Python 到 Erg 迁移的 Tips

## 要将字符串转换为 int 等

请使用类中的<gtr=“5”/>方法。它返回类型<gtr=“6”/>。


```python
s: str
i: int = int(s)
```


```erg
s: Str
res: Result(Int, IntParseError) = s.parse Int
i: Int = res.unwrap()
f: Result(Float, FloatParseError) = s.parse Float
```

也可以使用方法。


```erg
s: Str
i: Int = Int.try_from(s).unwrap()
f: Float = Float.try_from(s).unwrap()
```