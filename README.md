# 🍭 Candy

A sweet, functional programming language that is robust, minimalistic, and expressive.

Many programming languages have a strict separation between compile-time and runtime errors.
Sometimes, this border can seem arbitrary:
Dividing by a string fails during compilation with a type error, while dividing by zero only fails during runtime.
In the mathematical sense, there's no fundamental difference between these cases – division is only defined for divisors that are non-zero numbers.
That's why we eliminate the border between compile-time and runtime errors – all errors are runtime errors.
By crafting high-quality tooling with dynamic analyses such as fuzzing, we try to still be able to show most errors while you're editing the code.
In fact, we try to show more errors than typical statically typed languages.

![Candy in VS Code](screenshot.png)

## Quick introduction

- **Values are at the center of your computations.**
  Only a handful of predefined types of values exist:

  ```candy
  3                   # int
  "Candy"             # text
  Green               # symbol (uppercase)
  (Foo, Bar)          # list
  [Name: "Candy"]     # struct
  { it -> add it 2 }  # function
  ```

- **Minimalistic syntax.**
  Defining variables and functions works without braces or keywords cluttering up your code.
  The syntax is indentation-aware.

  ```candy
  # I'm a comment
  foo = 42                # variable (lowercase)
  println message =       # function
    print message         # function call
    print "\n"
  println "Hello, world!"
  ```

- **Extensive compile-time evaluation.**
  Many values can already be computed at compile-time.
  In your editor, you'll see the results on the right side:

  ```candy
  foo = double 2  # foo = 4
  ```

- **Fuzzing instead of traditional types.**
  In Candy, functions have to specify their needs _exactly._
  As you type, the tooling automatically tests your code with many inputs to see if one breaks the code:

  ```candy
  foo a =             # If you pass a = 0,
    needs (isInt a)
    math.logarithm a  # then this panics: The `input` must be a positive number.

  efficientTextReverse text =
    needs (isText text)
    needs (isPalindrome text) "Only palindromes can be efficiently reversed."
    text

  greetBackwards name =                   # If you pass name = "Test",
    "Hello, {efficientTextReverse name}"  # then this panics: Only palindromes can be efficiently reversed.
  ```

To get a more in-depth introduction, read the [language document](language.md).

## The current state

We are currently implementing a first version of Candy in Rust.
We already have a language server that provides some tooling.

## Discussion

[Join our <img height="12" src="https://github.com/candy-lang/candy/assets/19330937/baf90ce0-1d0a-4279-92fa-1c360cd28b38"> Discord server.](https://discord.gg/5Vr4eAJ7gU)

## How to use Candy

1. Install [<img height="16" src="https://rust-lang.org/static/images/favicon.svg"> Rust](https://rust-lang.org): [rust-lang.org/tools/install](https://rust-lang.org/tools/install) (use rustup, since we use nightly features).
2. Install [<img height="16" src="https://code.visualstudio.com/favicon.ico"> Visual Studio Code (VS Code)](https://code.visualstudio.com): [code.visualstudio.com/download](https://code.visualstudio.com/download).
3. Clone this repo.
4. Open the workspace (`compiler.code-workspace`) in VS Code.
5. Install the recommended VSCode extensions.
6. Run `npm install` inside `vscode_extension/`.
7. Run the launch config “Run Extension (VS Code Extension)”.
8. In the new VS Code window that opens, you can enjoy 🍭 Candy :)

If you are working on the Rust code itself, you might want to disable the release mode for quicker compilation:
In the VS Code settings (JSON), add the following: `"candy.languageServerCommand": "cargo run --manifest-path <path-to-the-candy-folder>/compiler/cli/Cargo.toml -- lsp"`.
