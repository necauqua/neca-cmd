# neca-cmd

The parser of TPN script, the command syntax of my Twitch chat bot. It was
first made for [Twitch Plays Noita](https://necauq.ua/posts/twitch-plays-noita).

The full syntax is in the
[TPN script reference](https://necauq.ua/tpn-script-reference).
To see how a message parses, use the [syntax debugger](https://uq.rs/lexer),
which runs this crate as WASM.

## Usage

```rust
use neca_cmd::Statement;

let statement = Statement::parse("hi, left~ wait:200~ right~ | +up");

assert_eq!(statement.to_string(), "left~ wait:200~ right~ | up~");
// `pure` is false, because the message also has text that is not a command
assert!(!statement.pure);
```

`Statement::parallel` holds one `Vec<Command>` for each group that is
separated by `|`. The commands in a group run one after another; the groups
run in parallel. The crate only parses; the caller runs the commands.

## Syntax, in short

- `cmd~`, `+cmd`, `~cmd`, `!cmd`, `?cmd` and `#cmd` are all the same command.
  A word without a symbol is a command only if it has params: `cmd:arg`.
- Params follow the name, separated by `:`: `mouse:123:321~`.
- String params: `"..."` (exact) or `{ ... }` (balanced and trimmed, `{ {} and
  {{}} }` is equivalent to `"{} and {{}}"`). `\` escapes a char.
- Math params: `(2 + 2 * 2)`. `calc::Calculator` evaluates them.
- Variables in params: `%name`, `%1`, `%(name)`, `%(name:default)`; `%%` is
  a literal `%`. `Param::expand` replaces them.
- A message that starts with `!` is one command with space-separated params:
  `!sr https://youtu.be/dQw4w9WgXcQ`.
- `Token::split_inline_number` splits `command123` into `command` and `123`,
  for the `command:123` shorthand.

Chatterino and the 7TV extension add `U+E0000` (old versions) or `U+034F` to the
end of a repeated message, to get around the duplicate message check of Twitch.
The parser ignores these suffixes.

## License

MIT
