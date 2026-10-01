# Favorite Launcher

Windows用のお気に入りボードです。Web履歴とローカル・ネットワークフォルダを検索し、よく使うリンクをグループカードで整理できます。

## 普段の使い方

- グループの取っ手をつかむと、中のリンクをまとめて並べ替え・別タブへ移動できます。
- 名前をクリックすると折り畳み、ダブルクリックすると名前変更できます。
- 設定の「お気に入りの列数」で1～10列を指定します。各列は等幅で上詰め、列ごとにスクロールします。グループの「⋯」から色、表示列、移動先タブを設定できます。
- 新規登録は検索下の「追加されたお気に入り」に入ります。ドラッグで右側へ配置します。「整理」を押すと、登録済みリンクの編集・削除・移動先選択が表示されます。
- お気に入りの検索は保管用タブも含む全タブが対象です。
- フォルダ検索は文書中心です。「すべて」または「その他…」で対象種類を切り替えられます。生成用フォルダ（node_modules、targetなど）は走査対象外です。
- Edge連携が止まった場合は最終受信時刻と接続確認を利用してください。Edgeが閉じている場合も応答なしと表示されます。

## 更新時

初回起動時、既存の見出しとその下のリンクを元の列内のグループへ変換します。リンクは削除しません。変換前のDBは同じデータ保存場所に `before-board-日時.db` として退避します。グループの表示列・折り畳み状態・ファイル種類はDBに保存され、バックアップにも含まれます。列数を減らしても内容は残り、非表示になる列のグループを最後の表示列へまとめます。列数を戻すと元の位置に表示されます。

## プロジェクト構成

ソースはルート直下の `src`、`src-tauri`、`public`、`edge-extension`。ビルド途中のファイルは `work`、配布ファイルは `outputs` へ保存します。

```powershell
npm install
npm run dev
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml board_tests --lib
npm run test:extension
npm run build:release
```

`build:release` はEXE、Setup EXE、MSI、拡張機能ZIPを `outputs` にコピーします。MSI検証にはWindows Installerサービスへアクセスできる実行環境が必要です。

拡張機能だけを梱包する場合は `npm run package:extension` を実行します。ストア登録については `edge-extension/STORE-SUBMISSION.md` を参照してください。ストア申請・公開は別途必要です。
