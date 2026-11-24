# Slice-n-Shuffle

画像を `x_split × y_split` 個のブロックに分割し、ランダムに並べ替えるツールです。Rust 製で、CLI・WASM（ブラウザ）の両方で動きます。

## 仕組み

- **暗号化（encode）**: 画像を等サイズのブロックに分割し、乱数でシャッフルして並べ直します。
- **復号（decode）**: 並べ替えを逆算するのではなく、ブロック同士の境界の類似度（ピクセル差の総和）から元の並びを推定します。隣り合うブロックの境界が最も近い並びを、greedy 法で探します。

## 重要な注意

### シードや元画像は保存しません

- 暗号化で出力されるのは **並べ替え後の画像（PNG）だけ** です。
- 乱数シード、並べ替えの対応表、鍵、元画像は **一切保存しません**。
- そのため、復号は「並びの情報をもとに戻す」処理ではなく、**画像内容から並びを推定する処理** です。
  - 推定が正しければ元の画像に戻りますが、保証はありません。

## ビルド

Rust の stable toolchain が必要です。

```sh
cargo build --release
```

## 使い方（CLI）

### 暗号化

```sh
cargo run --release -- encode <INPUT> <X_SPLIT> <Y_SPLIT> [-o <OUTPUT>] [--crop <LEFT,RIGHT,TOP,BOTTOM>]
```

例:

```sh
cargo run --release -- encode pexels-pixabay-53594.jpg 8 8
# => pexels-pixabay-53594_encoded.png
```

- 画像の幅は `X_SPLIT` で、高さは `Y_SPLIT` で割り切れる必要があります。割り切れない場合はエラーになります。
- `-o` に既存のディレクトリを指定すると、そのディレクトリに `<元のファイル名>_encoded.png` として出力されます。ファイル名を指定した場合は、そのファイルに出力されます。
- `--crop` は暗号化の前に画像を切り抜きます。
- 分割数が多いほど処理時間が増えます。

### 復号

```sh
cargo run --release -- decode <INPUT> <X_SPLIT> <Y_SPLIT> [-o <OUTPUT>] [--crop <LEFT,RIGHT,TOP,BOTTOM>]
```

例:

```sh
cargo run --release -- decode pexels-pixabay-53594_encoded.png 8 8
# => pexels-pixabay-53594_encoded_decoded.png
```

- 復号には暗号化時と **同じ `X_SPLIT` / `Y_SPLIT`** が必要です。
- 出力ファイル名は `<入力のファイル名>_decoded.png` です。

## 使い方（Web UI）

ブラウザで暗号化を試せます（暗号化のみ。復号は Web UI からは使えません）。

1. WASM をビルドします（[wasm-pack](https://rustwasm.github.io/wasm-pack/) が必要です）。

   ```sh
   sh build_wasm.sh
   ```

   生成物は `public/pkg/` に出力されます。
2. `public/` を静的ファイルサーバーで配信し、`public/index.html` を開きます。
3. 画像と分割数を選び、Run を押します。

## ファイル構成

| パス | 内容 |
| --- | --- |
| `src/main.rs` | CLI（`encode` / `decode` サブコマンド、切り抜き処理） |
| `src/lib.rs` | 公開 API（`encode_image` / `decode_image`）と WASM のエクスポート |
| `src/sns.rs` | 分割・並べ替え・復号（greedy 探索） |
| `src/similarity.rs` | ブロック境界のピクセル差の計算 |
| `src/random.rs` | シャッフル用の乱数生成（実行ごとに OS の乱数で初期化） |
| `src/error.rs` | エラー型 |
| `public/` | Web UI |
| `build_wasm.sh` | WASM ビルドスクリプト |

## ライセンス

[LICENSE](LICENSE) を参照してください。
