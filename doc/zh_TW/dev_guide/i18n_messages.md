# Multilingualization of Messages

Erg 正在推動消息（開始、選項、文檔、提示、警告、錯誤消息等）的多語言化。如果你不熟悉 Rust 或 Erg，也可以參與此項目。請務必配合。

以下是多語種方法的說明。

## 查找

在 Erg 源代碼中找到（使用 grep 或編輯器的搜索功能）。我們應該能找到下面這樣的東西。


```rust
switch_lang!(
    "japanese" => format!("この機能({name})はまだ正式に提供されていません"),
    "english" => format!("this feature({name}) is not implemented yet"),
),
```

此消息目前僅支持日語和英語。讓我們嘗試添加簡體消息。

## 添加消息

請在查看其他語言內容的同時添加翻譯消息。最後不要忘記逗號（）。


```rust
switch_lang!(
    "japanese" => format!("この機能({name})はまだ正式に提供されていません"),
    "simplified_chinese" => format!("該功能({name})還沒有正式提供"),
    "english" => format!("this feature({name}) is not implemented yet"),
),
```

另外，英語是默認設置，一定要排在最後。部分是 Rust 的格式化功能，允許你將變量的內容（<gtr=“7”/>）嵌入到字符串中。

## Build

現在，我們使用選項構建它。

<img src="../../../assets/screenshot_i18n_messages.png" alt='screenshot_i18n_messages'>

你做到了！

## FAQ

Q：像這樣的指定是什麼意思？ A：{RED} 及更高版本將顯示為紅色。重新啟動交互渲染。

Q：如果想添加自己的語言，該如何替換部分？答：目前支持以下語言。

* english（默認設置）
* 日語。
* 簡體中文
* 繁體中文

如果你想添加其他語言，請提出請求。