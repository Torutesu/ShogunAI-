import type { MarketingDetail } from './marketing-content';

const jaFeatures: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory',
    eyebrow: 'AIメモリ',
    title: '働いているところを見て、あなたの仕事を覚える',
    description: 'macOS がすでにスクリーンリーダー向けに公開しているテキストを読み、出どころと時刻とともに Mac の中に暗号化して保存し、端末上でインデックスします。',
    intro: '一日のうち本当に効いている部分は、ひとつの文書には収まりません。流し読みしたスレッド、口頭で下した判断、書きかけでやめた下書き、誰かが直したドキュメント。ShogunAI はそれを、あなたが維持しなくても残る記憶として持ちます ── そして、何を持っているかをあなたが確認できる場所に置きます。',
    highlights: [
      { title: 'まずはアクセシビリティ経由のテキスト', body: 'フォーカス中のウィンドウのアプリ名・タイトル・可視テキストを、スクリーンリーダーと同じ読み方で取得します。除外アプリ、プライベートウィンドウ、パスワード欄は読みません。一時停止で完全に止まります。これらの除外は保存済みデータへのフィルタではなく、書き込みの前に適用されます。' },
      { title: 'スクリーンショットは、テキストが取れないときだけ', body: 'ビジュアルリコールは既定でオフです。オンにすると、テキストが一切取れなかったウィンドウにかぎり圧縮したスクリーンショットを取得します ── 共有スライドのグラフ、キャンバス描画のダッシュボード、スキャンされただけのPDF。Mac 上で暗号化され、あなたが選んだ保持期間（1〜7日、または上限つきのカスタム）で自動的に削除されます。' },
      { title: 'インデックスは Mac の上で', body: '多言語の埋め込みモデルを同梱しているので、濃い一日をインデックスしても追加費用はゼロで、想起はネットワークなしで動きます。あなた自身の記憶にトークン単位のメーターは回りません。だから私たちは「安くするために、あなたのことを覚える量を減らす」必要が一度もありません。' },
      { title: 'すべての記憶が出どころを持つ', body: '各エントリはそれを生んだイベントに紐づきます。だから想起は「火曜14:20、設計ドキュメントの中で」と答えられます。そして、確信度の低い推測があなたの送るものに混ざらないのも、この仕組みのおかげです。' },
    ],
    steps: [
      { title: '一度だけ権限を許可する', body: 'macOS が尋ね、あなたが許可すると、実際に作業しているウィンドウに追随します。除外したアプリからは何も読みません。' },
      { title: 'いつもどおり働く', body: 'メモを取らなくても文脈が溜まります。直近1日は即座に、直近1か月は詳細のまま、それより古いものは圧縮して保持されます。' },
      { title: '中身を見て、削る', body: 'エントリを消すと、検索からも、階層化された保存からも、あとで組み立てられる文脈からも消えます。一時停止はいつでも。取得しなかったものは、保存もされません。' },
    ],
    outcomes: ['数週間後に、判断の理由まで思い出せる', '数字の根拠を、導き直さずに見つけられる', '中断した仕事に、経緯を組み立て直さずに戻れる', '機微な文脈を既定で端末内に保てる'],
    faq: [
      ['ShogunAI はスクリーンショットを撮りますか?', 'テキストが取れなかった画面にかぎり、かつビジュアルリコールを自分でオンにした場合だけ撮ります ── 既定はオフです。撮ったものは Mac 上で暗号化され、いつでも閲覧・削除でき、選んだ保持期間で自動削除されます。それ以外の場面ではアクセシビリティ経由のテキストのみで、画像は書き込みません。'],
      ['会議の音声は保存されますか?', 'いいえ。会議中に文字起こしのためだけに処理し、ディスクにも一時ファイルにも書きません。残るのは文字起こしテキストとその出どころだけです。'],
      ['記憶はどこにありますか?', 'あなたの Mac の暗号化されたデータベースの中です。差し押さえたり、侵害したり、こっそり学習に使えるサーバー側のコピーは存在しません。外に出るのは、その依頼に必要な範囲だけで、目的とともに記録されます。'],
      ['濃い一日はコストが上がりますか?', 'いいえ。埋め込みは端末上で動くので、量に関係なくインデックスは無料で、想起はオフラインでも動きます。'],
    ],
  },
  {
    slug: 'contextual-recall',
    eyebrow: '文脈検索',
    title: '経緯を貼り直さずに、今週のことを尋ねる',
    description: 'メール・チャット・ドキュメント・カレンダーに、ひとつの問いで。答えはあなた自身の履歴から返り、すべてのヒットに出どころと時刻が付きます。',
    intro: '汎用のアシスタントは毎回空の状態から始まるので、まともな質問にたどり着く前に自分の状況を説明し直す数分を使います。ShogunAI はすでに作られた記憶から始まるので、質問は実際に頭に浮かんだ短いままで済みます。',
    highlights: [
      { title: '覚えているとおりに尋ねる', body: '人、プロジェクト、決定、だいたいの時期で。完全一致のキーワードだけではありません。全文検索とベクトル検索を同時に走らせるので、半端に覚えている言い回しでも、曖昧な説明でも届きます。' },
      { title: 'ひとつの問いで、つないだツール全部に', body: 'メール、チャット、ドキュメント、カレンダーを4回ではなく1回の検索で。許可したものが対象で、許可していないものは対象外のままです。' },
      { title: '確かめられる答え', body: 'すべてのヒットに出どころと時刻が付きます。「火曜14:20、設計ドキュメントの中で」と返り、そこを開けます。根拠のない段落を信じる必要はありません。' },
      { title: '確信度が見える', body: 'システムが持つ状態 ── 人、プロジェクト、約束、開いたループ ── はそれぞれ確信度を持ちます。確信度の低い読みは問いとして表に出て、あなたが送ろうとしているものの中で事実として固まることはありません。' },
    ],
    steps: [
      { title: '尋ねる', body: '一部しか覚えていなくても、普通の言葉で必要なものを説明します。' },
      { title: '引き出す', body: 'Warm層 ── 直近1か月を詳細のまま ── を検索し、出どころを付けて返します。' },
      { title: '続ける', body: '答えをブリーフ、返信、次のアクションに変えます。文脈をやり直す必要はありません。' },
    ],
    outcomes: ['会議に入る前に、前回どこで終わったかを把握している', '2週間空けたあとでも決定を取り戻せる', 'ひとつのスレッドのために4つのアプリを探さなくなる', '記憶ではなく事実から下書きする'],
    faq: [
      ['ふつうの検索と何が違いますか?', 'ふつうの検索は完全一致の言葉と、アプリごとの検索を前提にします。文脈検索は、つないだツールを横断して意味で引き出し、結果ごとに出どころと時刻を添えるので、答えを確かめられます。'],
      ['オフラインでも動きますか?', '想起は動きます。インデックスも検索も Mac 上で動くので、見つけるのにネットワークは要りません。ネットワークが必要なのは生成 ── 返信を書く、難しい問いを考える ── の部分です。'],
      ['確信が持てない場合は?', '確信度の低い状態は断定されず、はぐらかされます。「ミカに修正した数字をまだ返していない可能性があります」として出るのであって、下書きの中に静かに固まったりはしません。'],
      ['どのプランに含まれますか?', 'Standard です。キャプチャ、想起、デイリーブリーフ、第1層の読み取り連携はすべて下位プランに含まれます。'],
    ],
  },
  {
    slug: 'execution-layer',
    eyebrow: '実行レイヤー',
    title: '答えるほうが、簡単な半分です',
    description: 'いま使っているツールの中で下書き・更新・操作を行います。名前のついた3段階の自律性と、ほかの人に届く前の停止つきで。',
    intro: 'たいていのメモリ製品はよく答え、仕事はあなたに残します ── 下書きをコピーし、メールを開き、宛先を直し、ファイルを探し、添付し、送る。実行レイヤーはその隙間を埋める部分で、そこに効いているルールは動き出す前に確認できます。',
    highlights: [
      { title: 'Option を押せば、キャレットに書かれる', body: 'カーソル周りのフィールドと、その背後にある記憶を読んで、いま入力しているアプリのキャレットに直接続きを書きます。これは端末内の書き込みです。送信はしません。送るのはあなたです。' },
      { title: '3段階、そしてその線は動かない', body: '第1段階は取り消せてローカルに閉じ、そのまま実行されます。第2段階は下書きされ、あなたを待ちます。第3段階はほかの人に届くものすべて ── メールの送信、メッセージの投稿、誰かのカレンダーへの予定作成 ── で、必ず承認で止まります。プロンプトでアクションの段階が動くことはありません。' },
      { title: 'あなたのプラン、またはあなたの鍵', body: 'すでに払っているアシスタントのプランの枠内でも、持ち込んだ API キーでも動きます。提供者を選ぶのはあなたで、乗り換えても記憶は1日も失われません。キーはシステムのキーチェーンにだけ保存されます。' },
      { title: '何をしたかの記録', body: 'すべてのアクションが、何が、どんな根拠で実行され、何が端末を出たかを残します。第三者を経由した場合はそう明記されます。このログこそが第1段階を許容可能にしているものです ── 事後に監査できる自動処理と、事前に信じるしかない自動処理は、まったく別の提案です。' },
    ],
    steps: [
      { title: '理解する', body: 'スレッドだけでなく、仕事の状態 ── 誰が関わり、何が決まり、何が開いたままか ── に照らして依頼を読みます。' },
      { title: '用意する', body: '正しいバージョンのファイルを添付し、未回答の問いに答えた状態で、下書き・更新・ツール操作を組み立てます。' },
      { title: '承認する', body: '影響のあるものは承認ひとつを待ちます。取り消せる処理は、あなたが見る頃には終わっています。' },
    ],
    outcomes: ['正しいバージョンを添えて follow-up を送れる', '15時の打ち合わせに、準備が済んだ状態で入れる', '開いていたことを忘れていたループが閉じる', '送信ボタンは自分の手に残る'],
    faq: [
      ['勝手に送信することはありますか?', 'ありません。ほかの人に届くものはすべて第3段階で、承認で止まります。これはあなたが正しく設定してくれることを願う設定項目ではなく、アクションの経路そのものの性質です。'],
      ['API から動かせばゲートを回避できますか?', 'できません。MCP・CLI・REST でも同じ分類器と同じ関門が適用されます。外部から呼び出すエージェントに、あなた自身のクリック以上の権限はありません。'],
      ['API キーは必要ですか?', 'いいえ。すでに払っているアシスタントのプランの上で動かせます。自分のキーを持ち込むのは代替手段であって、必須条件ではありません。'],
      ['どのプランに含まれますか?', 'Pro です。Memory API と第2層の連携も同じく Pro。Standard はキャプチャ、想起、日々の実行までをカバーします。'],
    ],
  },
];

const jaUseCases: readonly MarketingDetail[] = [
  {
    slug: 'founders', eyebrow: '創業者向け', title: '失う余裕のない会社の文脈を、確実に残す',
    description: '意思決定を思い出し、投資家・チーム向け更新を準備し、散らばった会社の文脈を次の行動へつなげます。',
    intro: '創業者は、プロダクト、採用、顧客、資金調達、経営を一日の中で何度も切り替えます。ShogunAIは、その切り替えの裏にある経緯を残し、次の会話を過去の続きから始められるようにします。',
    highlights: [
      { title: '投資家対応の準備', body: '過去の報告、指標に関する議論、未回答の質問、約束事項を一つの準備フローにまとめます。' },
      { title: '意思決定の履歴', body: '最終的なタスクや文書だけでなく、なぜその方向を選んだのかまで取り戻せます。' },
      { title: '確実なフォロー', body: '会議や会話を、下書き、進捗報告、担当が明確な次のアクションへ変えます。' },
    ],
    steps: [
      { title: '経営の文脈を蓄積', body: '会社を形づくるプロジェクト、会話、調査を継続的な記憶にします。' },
      { title: '判断前に確認', body: '過去の約束、反対意見、前提条件を自然な言葉で呼び出します。' },
      { title: 'フォローを完了', body: '同じ文脈から更新や返信を準備し、確認して実行します。' },
    ],
    outcomes: ['取締役会・投資家向け更新を準備する', '資金調達の会話を正確に再開する', '候補者ごとの採用文脈を保つ', '経営者のコンテキストスイッチを減らす'],
    faq: [
      ['社内ナレッジベースの代わりになりますか？', '主に個人のためのプライベートな記憶・実行レイヤーです。共有ナレッジを置き換えるのではなく、自分の仕事の背景を補完します。'],
      ['投資家との会議準備にも使えますか？', 'はい。過去の議論、質問、約束、関連作業を取り出し、根拠のあるブリーフを作れます。'],
      ['チーム全員がツールを変える必要がありますか？', 'ありません。既存ツールを横断して働く設計なので、ワークフロー全体の移行は不要です。'],
    ],
  },
  {
    slug: 'product-engineering', eyebrow: 'プロダクト・開発向け', title: '議論から提供まで、プロダクトの文脈を途切れさせない',
    description: '技術的な判断、顧客の根拠、デザイン上のトレードオフ、プロジェクト履歴を、ツールごとに探し回らず取り出せます。',
    intro: '顧客要望はメール、判断はSlack、デザインはFigma、課題はLinearというように、プロダクトの文脈はツールの境界で失われます。ShogunAIは、それらの出来事を横断する個人の文脈レイヤーをつくります。',
    highlights: [
      { title: '判断理由の検索', body: 'プロダクトや技術選定の背景にある理由、代案、制約まで見つけられます。' },
      { title: '引き継ぎの高速化', body: 'すべての情報源を手作業で再構成せず、必要な文脈を簡潔にまとめられます。' },
      { title: 'プロジェクトの継続性', body: '中断した仕事へ、関連履歴と次の工程を確認した状態ですぐに戻れます。' },
    ],
    steps: [
      { title: '作業の流れを残す', body: '調査、議論、設計、実装の間を移動する文脈を取得します。' },
      { title: '「なぜ」を取り出す', body: '機能、不具合、顧客、判断について、チームで使う言葉のまま質問します。' },
      { title: '成果物に変える', body: '文脈をブリーフ、課題、進捗報告、引き継ぎ資料へ変えて確認します。' },
    ],
    outcomes: ['より良いプロダクトブリーフを書く', '技術判断の根拠を取り戻す', 'スプリント・リリース報告を準備する', '過去のプロジェクトへ素早く復帰する'],
    faq: [
      ['Linear、Jira、Notionの代わりですか？', 'いいえ。既存ツールを置き換える管理システムではなく、その間をつなぐ文脈・実行レイヤーです。'],
      ['コードとプロダクトの文脈を関連付けられますか？', '許可したツールを横断し、実装の周辺にある議論や成果物を関連付ける設計です。'],
      ['個人開発者にも役立ちますか？', 'はい。エンジニア、デザイナー、プロダクトマネージャーを含む、個人の仕事の記憶を中心に設計しています。'],
    ],
  },
  {
    slug: 'consultants', eyebrow: 'コンサルタント向け', title: 'すべての顧客を覚えながら、頭の中だけに抱え込まない',
    description: '顧客ごとの文脈を保ち、準備時間を短縮し、すでに完了した仕事に基づいてフォローを作成します。',
    intro: '顧客業務では、会社、人、専門用語、約束事項を素早く切り替える必要があります。ShogunAIは、打ち合わせ前に正しい文脈を取り戻し、終了後は成果物へつなげます。',
    highlights: [
      { title: '顧客文脈の検索', body: '顧客・案件ごとに、過去の会話、制約、成果物、未解決の質問を取り出せます。' },
      { title: '会議準備', body: 'すべてのメッセージや文書を読み直さず、最近の仕事から要点をまとめます。' },
      { title: '一貫したフォロー', body: '実際に話した内容を根拠に、要約と次のアクションを下書きします。' },
    ],
    steps: [
      { title: '顧客ごとの記憶をつくる', body: '個人の仕事レイヤーをローカルファーストに保ちながら必要な文脈を蓄積します。' },
      { title: '打ち合わせ前に準備', body: '最近の変更、約束、未決事項を一度の質問で確認します。' },
      { title: '打ち合わせ後に届ける', body: '明確な議事録、計画、顧客向け下書きを作り、送信前に確認します。' },
    ],
    outcomes: ['顧客切り替えの負担を減らす', '精度の高い会議ブリーフを作る', '約束事項の見落としを減らす', '要約や提案書を早く下書きする'],
    faq: [
      ['顧客ごとの文脈を分けて管理できますか？', '取得範囲と連携サービスを自分で管理し、必要な仕事の文脈を検索できるようにする設計です。'],
      ['顧客メールが自動送信されますか？', '顧客向けの送信など重要な操作には承認を挟み、実行前に内容を確認できます。'],
      ['記憶はクラウドだけに保存されますか？', 'いいえ。既定ではローカルファーストで、機能上必要な場合だけ承認したプロバイダへ関連情報を共有します。'],
    ],
  },
];

const esFeatures: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory',
    eyebrow: 'Memoria de IA',
    title: 'Aprende tu trabajo viéndote trabajar',
    description: 'ShogunAI lee el texto que macOS ya expone a los lectores de pantalla, lo guarda cifrado en tu Mac con su origen y su hora, y lo indexa en el dispositivo.',
    intro: 'La parte útil de una jornada nunca está en un solo documento. Está repartida entre un hilo que ojeaste, una decisión que tomaste en voz alta, un borrador que abandonaste y un documento que editó otra persona. ShogunAI conserva eso como una memoria que no tuviste que mantener, y la deja donde puedes ver exactamente qué contiene.',
    highlights: [
      { title: 'Primero texto, por la capa de accesibilidad', body: 'La app, el título y el texto visible de la ventana enfocada, leídos como los lee un lector de pantalla. Las apps excluidas, las ventanas privadas y los campos de contraseña no se leen nunca, y la pausa detiene la captura por completo. Esas exclusiones se aplican antes de escribir nada, no como un filtro sobre datos ya guardados.' },
      { title: 'Capturas solo donde el texto falla', body: 'El recuerdo visual está desactivado hasta que tú lo actives. Con él activo, se toma una captura comprimida solo en una ventana que no entrega texto alguno: un gráfico en una diapositiva compartida, un panel dibujado en canvas, un PDF escaneado. Queda cifrada en el Mac y se borra sola al cumplir el periodo de retención que elijas, de uno a siete días o una duración personalizada acotada.' },
      { title: 'Indexado en tu Mac', body: 'La app incluye un modelo de embeddings multilingüe, así que indexar un día denso no cuesta nada extra y la recuperación funciona sin red. No hay un contador por token sobre tu propia memoria, y por eso nunca necesitamos recordar menos de tu día para abaratar el producto.' },
      { title: 'Cada recuerdo conserva su fuente', body: 'Las entradas enlazan al evento que las originó. Eso permite responder «martes 14:20, en el documento de diseño» en vez de una suposición segura de sí misma, y mantiene fuera de lo que envías cualquier lectura poco fiable.' },
    ],
    steps: [
      { title: 'Concede accesibilidad una vez', body: 'macOS pregunta, tú apruebas, y la captura sigue la ventana en la que realmente trabajas. De las apps excluidas no se lee nada.' },
      { title: 'Trabaja como siempre', body: 'El contexto se acumula sin tomar notas. El último día queda disponible al instante, el último mes con todo el detalle, y lo más antiguo comprimido.' },
      { title: 'Revisa y depura', body: 'Al borrar una entrada desaparece de la búsqueda, del almacenamiento y del contexto que se prepare después. Pausa cuando quieras: lo que no se captura no se guarda.' },
    ],
    outcomes: ['Recuperar el razonamiento de una decisión semanas después', 'Encontrar el origen de una cifra en vez de rehacerla', 'Volver al trabajo interrumpido sin reconstruir la historia', 'Mantener el contexto sensible en el dispositivo por defecto'],
    faq: [
      ['¿ShogunAI hace capturas de pantalla?', 'Solo donde la captura de texto no devuelve nada, y solo si activas el recuerdo visual: viene desactivado. Esas capturas quedan cifradas en tu Mac, puedes verlas o borrarlas cuando quieras, y se eliminan solas al cumplir el periodo que elijas. En el resto de los casos la captura es texto por la capa de accesibilidad y no se escribe ninguna imagen.'],
      ['¿Se guarda el audio de las reuniones?', 'No. El audio se procesa para la transcripción mientras la reunión ocurre y nunca se escribe en disco ni en un archivo temporal. Lo que persiste es el texto de la transcripción y su origen.'],
      ['¿Dónde vive la memoria?', 'En una base de datos cifrada en tu Mac. No existe una copia de tu día en un servidor que embargar, filtrar o usar para entrenar en silencio. Lo que sale es la porción concreta que necesita una petición, registrada con su propósito.'],
      ['¿Un día intenso cuesta más?', 'No. Los embeddings corren en local, así que indexar es gratis sea cual sea el volumen, y la recuperación funciona sin conexión.'],
    ],
  },
  {
    slug: 'contextual-recall',
    eyebrow: 'Recuperación contextual',
    title: 'Pregunta por tu semana sin pegar los antecedentes',
    description: 'Una pregunta sobre correo, chat, documentos y calendario, respondida desde tu propio historial con la fuente y la hora en cada resultado.',
    intro: 'Un asistente genérico empieza cada sesión vacío, así que gastas los primeros minutos volviendo a explicar tu vida antes de poder preguntar algo útil. ShogunAI parte de la memoria que ya construyó, de modo que la pregunta puede ser la corta que de verdad tenías.',
    highlights: [
      { title: 'Pregunta como lo recuerdas', body: 'Por persona, proyecto, decisión o más o menos cuándo, no solo por palabra exacta. La búsqueda híbrida ejecuta texto completo y vectores a la vez, así que llega tanto una frase a medio recordar como una descripción difusa.' },
      { title: 'Una pregunta, todas las herramientas conectadas', body: 'Correo, chat, documentos y calendario resueltos con una búsqueda en lugar de cuatro. Lo que autorizaste entra; lo que no, se queda fuera.' },
      { title: 'Respuestas que puedes comprobar', body: 'Cada resultado lleva su fuente y su hora. Obtienes «martes 14:20, en el documento de diseño» y puedes abrirlo, en vez de confiar en un párrafo sin nada detrás.' },
      { title: 'La confianza es visible', body: 'Cada pieza de estado —personas, proyectos, compromisos, ciclos abiertos— lleva un nivel de confianza. Una lectura poco fiable aparece como pregunta, nunca como un hecho dentro de algo que estás a punto de enviar.' },
    ],
    steps: [
      { title: 'Pregunta', body: 'Describe lo que necesitas en lenguaje corriente, aunque solo recuerdes una parte.' },
      { title: 'Recupera', body: 'La búsqueda va al nivel templado —el último mes con todo el detalle— y devuelve resultados con su procedencia.' },
      { title: 'Continúa', body: 'Convierte la respuesta en un resumen, una respuesta o una siguiente acción sin rehacer el contexto.' },
    ],
    outcomes: ['Entrar a una reunión sabiendo dónde quedó', 'Recuperar una decisión tras dos semanas fuera', 'Dejar de buscar un hilo en cuatro apps', 'Redactar desde hechos y no desde la memoria'],
    faq: [
      ['¿En qué se diferencia de una búsqueda?', 'La búsqueda espera las palabras exactas y una app cada vez. La recuperación trae por significado a través de las herramientas que conectaste, y adjunta fuente y hora a cada resultado para que la respuesta sea comprobable.'],
      ['¿Funciona sin conexión?', 'Para recuperar, sí. El índice y la búsqueda corren en tu Mac, así que encontrar cosas no necesita red. La generación —redactar una respuesta, razonar una pregunta difícil— es la parte que necesita un modelo.'],
      ['¿Y si no está seguro?', 'El estado poco fiable se matiza en lugar de afirmarse. Vuelve como «puede que no le hayas enviado a Mika las cifras revisadas», no como una frase endurecida dentro de un borrador.'],
      ['¿En qué plan está?', 'En Standard. Captura, recuperación, resumen diario y las conexiones de lectura de primera capa están todas en el plan inferior.'],
    ],
  },
  {
    slug: 'execution-layer',
    eyebrow: 'Capa de ejecución',
    title: 'Responder es la mitad fácil',
    description: 'Borradores, actualizaciones y acciones dentro de las herramientas que ya usas, bajo tres niveles de autonomía con nombre y una parada antes de que algo llegue a otra persona.',
    intro: 'La mayoría de los productos de memoria responden bien y te dejan el trabajo: copiar el borrador, abrir el correo, corregir el destinatario, buscar el archivo, adjuntarlo, enviarlo. La capa de ejecución es la parte que cierra ese hueco, y sus reglas se ven antes de que algo se ejecute.',
    highlights: [
      { title: 'Pulsa Option y escribe en tu cursor', body: 'La composición en línea lee el campo alrededor del cursor y la memoria que hay detrás, y escribe la continuación directamente en la app en la que ya estás escribiendo. Es una escritura local: no se envía nada, y el envío lo haces tú.' },
      { title: 'Tres niveles, y la línea no se mueve', body: 'El nivel uno es reversible y local, y se ejecuta solo. El nivel dos se redacta y te espera. El nivel tres es todo lo que llega a otra persona —enviar correo, publicar un mensaje, poner un evento en el calendario de alguien— y siempre se detiene para tu aprobación. Ninguna instrucción mueve una acción de nivel.' },
      { title: 'Tu plan o tu clave', body: 'La ejecución corre con la suscripción de asistente que ya pagas, dentro de los límites de ese plan, o con una clave de API que traigas. Eliges el proveedor y puedes cambiarlo sin perder un día de memoria. Las claves viven en el llavero del sistema y en ningún otro sitio.' },
      { title: 'Un registro de lo que hizo', body: 'Cada acción deja qué se ejecutó, con qué evidencia y qué salió del dispositivo, marcado cuando pasó por un tercero. Ese registro es lo que hace aceptable el nivel uno: una automatización que puedes auditar después es una propuesta distinta de una en la que tienes que confiar por adelantado.' },
    ],
    steps: [
      { title: 'Entender', body: 'La petición se lee contra el estado de tu trabajo —quién participa, qué se decidió, qué sigue abierto— y no solo contra el hilo.' },
      { title: 'Preparar', body: 'El borrador, la actualización o la acción se montan con el archivo correcto ya adjunto y la pregunta abierta ya respondida.' },
      { title: 'Aprobar', body: 'Lo importante espera una aprobación. Lo reversible ya ha terminado cuando lo miras.' },
    ],
    outcomes: ['Enviar el seguimiento con la versión correcta adjunta', 'Llegar preparado a la reunión de las 15:00', 'Cerrar el ciclo que habías olvidado abierto', 'Mantener el botón de enviar en tus manos'],
    faq: [
      ['¿Puede enviar algo sin preguntar?', 'No. Todo lo que llega a otra persona es nivel tres y se detiene para tu aprobación. Es una propiedad de cómo se enrutan las acciones, no un ajuste que debas configurar bien.'],
      ['¿Usarlo por API se salta el control?', 'No. El mismo clasificador y los mismos controles se aplican por MCP, CLI y REST. Un agente que llama desde fuera no tiene más autoridad que tu propio clic.'],
      ['¿Necesito una clave de API?', 'No. Puede funcionar con el plan de asistente que ya pagas. Traer tu propia clave es la alternativa, no el requisito.'],
      ['¿En qué plan está?', 'En Pro, junto con la Memory API y las conexiones de segunda capa. Standard cubre captura, recuperación y ejecución cotidiana.'],
    ],
  },
];

const esUseCases: readonly MarketingDetail[] = [
  { slug: 'founders', eyebrow: 'Para fundadores', title: 'Conserva el contexto de la empresa que no puedes perder', description: 'Recuerda decisiones, prepara actualizaciones para inversores y equipos, y convierte contexto disperso en acción.', intro: 'Un fundador cambia constantemente entre producto, contratación, clientes, financiación y operaciones. ShogunAI mantiene disponible el contexto de esos cambios.', highlights: [{ title: 'Preparación para inversores', body: 'Reúne informes anteriores, métricas, preguntas abiertas y compromisos.' }, { title: 'Historial de decisiones', body: 'Recupera por qué el equipo eligió una dirección, no solo el resultado final.' }, { title: 'Seguimiento', body: 'Convierte reuniones en borradores, actualizaciones y siguientes pasos claros.' }], steps: [{ title: 'Captura el contexto operativo', body: 'Crea memoria entre proyectos, conversaciones e investigación.' }, { title: 'Pregunta antes de decidir', body: 'Recupera compromisos, objeciones y supuestos.' }, { title: 'Completa el seguimiento', body: 'Prepara y aprueba la actualización desde el mismo contexto.' }], outcomes: ['Preparar actualizaciones para inversores', 'Retomar conversaciones de financiación', 'Mantener contexto de contratación', 'Reducir cambios mentales de contexto'], faq: [['¿Es una base de conocimiento empresarial?', 'Es una memoria y capa de ejecución privada para el individuo que complementa el conocimiento compartido.'], ['¿Ayuda con reuniones de inversores?', 'Sí. Recupera conversaciones, preguntas y compromisos para preparar un briefing fundamentado.'], ['¿El equipo debe cambiar de herramientas?', 'No. Está diseñado para trabajar entre las herramientas existentes.']] },
  { slug: 'product-engineering', eyebrow: 'Para producto e ingeniería', title: 'Lleva el contexto de producto desde la discusión hasta la entrega', description: 'Recupera decisiones técnicas, evidencia de clientes, compromisos de diseño e historial del proyecto.', intro: 'El contexto se pierde entre correo, Slack, Figma y Linear. ShogunAI crea una capa personal que conecta esos momentos.', highlights: [{ title: 'Recuerdo de decisiones', body: 'Encuentra razones, alternativas y restricciones detrás de una elección.' }, { title: 'Traspasos más rápidos', body: 'Prepara contexto claro sin reconstruir manualmente cada fuente.' }, { title: 'Continuidad del proyecto', body: 'Vuelve al trabajo interrumpido con historial y siguientes pasos.' }], steps: [{ title: 'Seguir el trabajo', body: 'Captura contexto entre investigación, diseño e implementación.' }, { title: 'Recuperar el porqué', body: 'Pregunta por una función, error, cliente o decisión.' }, { title: 'Crear el artefacto', body: 'Convierte contexto en un brief, incidencia o actualización.' }], outcomes: ['Escribir mejores briefs', 'Recuperar razones técnicas', 'Preparar actualizaciones de sprint', 'Volver rápido a proyectos antiguos'], faq: [['¿Sustituye a Linear, Jira o Notion?', 'No. Es una capa de contexto y ejecución sobre tus herramientas actuales.'], ['¿Conecta código y contexto de producto?', 'Está diseñado para relacionar discusiones y artefactos alrededor de la implementación.'], ['¿Sirve a colaboradores individuales?', 'Sí. Está centrado en la memoria privada de ingenieros, diseñadores y PM.']] },
  { slug: 'consultants', eyebrow: 'Para consultores', title: 'Recuerda a cada cliente sin guardar todo en tu cabeza', description: 'Mantén contexto por cliente, prepárate más rápido y crea seguimientos basados en el trabajo realizado.', intro: 'La consultoría exige cambiar entre empresas, personas, términos y compromisos. ShogunAI recupera el contexto correcto antes de una llamada y ayuda a entregar después.', highlights: [{ title: 'Recuerdo del cliente', body: 'Recupera conversaciones, restricciones, entregables y preguntas por proyecto.' }, { title: 'Preparación de reuniones', body: 'Crea un briefing sin revisar cada mensaje y documento.' }, { title: 'Seguimiento consistente', body: 'Redacta resúmenes y próximos pasos basados en lo hablado.' }], steps: [{ title: 'Crear memoria de cliente', body: 'Captura el contexto necesario con un enfoque local-first.' }, { title: 'Preparar la llamada', body: 'Recupera cambios, compromisos y decisiones abiertas.' }, { title: 'Entregar después', body: 'Crea un resumen o plan y revísalo antes de enviarlo.' }], outcomes: ['Cambiar de cliente con menos carga', 'Crear mejores briefings', 'Reducir compromisos olvidados', 'Redactar propuestas más rápido'], faq: [['¿Puedo separar contextos de clientes?', 'Tú controlas qué se captura y qué servicios se conectan.'], ['¿Envía correos automáticamente?', 'Las acciones dirigidas a clientes requieren aprobación antes de enviarse.'], ['¿La memoria está solo en la nube?', 'No. Es local-first por defecto y solo comparte contexto cuando una función autorizada lo necesita.']] },
];

const deFeatures: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory',
    eyebrow: 'KI-Gedächtnis',
    title: 'Es lernt deine Arbeit, indem es dir bei der Arbeit zusieht',
    description: 'ShogunAI liest den Text, den macOS Screenreadern ohnehin bereitstellt, hält ihn samt Herkunft und Zeitpunkt verschlüsselt auf deinem Mac und indexiert ihn auf dem Gerät.',
    intro: 'Der nützliche Teil eines Arbeitstags steckt nie in einem einzigen Dokument. Er verteilt sich auf einen überflogenen Thread, eine laut getroffene Entscheidung, einen abgebrochenen Entwurf und ein Dokument, das jemand anderes bearbeitet hat. ShogunAI bewahrt das als Gedächtnis, das du nicht pflegen musstest — und legt es dorthin, wo du genau sehen kannst, was darin steht.',
    highlights: [
      { title: 'Zuerst Text, über die Bedienungshilfen', body: 'App, Titel und sichtbarer Text des fokussierten Fensters, gelesen wie ein Screenreader liest. Ausgeschlossene Apps, private Fenster und Passwortfelder werden nie gelesen, und Pause stoppt die Erfassung vollständig. Diese Ausschlüsse greifen, bevor etwas geschrieben wird — nicht als Filter über bereits gespeicherte Daten.' },
      { title: 'Screenshots nur, wo Text scheitert', body: 'Visual Recall ist aus, bis du es einschaltest. Ist es an, entsteht ein komprimierter Screenshot nur bei einem Fenster, das überhaupt keinen Text liefert: ein Diagramm in einer geteilten Folie, ein auf Canvas gezeichnetes Dashboard, ein eingescanntes PDF. Er bleibt verschlüsselt auf dem Mac und löscht sich bei der von dir gewählten Aufbewahrungsdauer selbst — ein bis sieben Tage oder eine begrenzte eigene Dauer.' },
      { title: 'Indexiert auf deinem Mac', body: 'Ein mehrsprachiges Embedding-Modell liegt der App bei, also kostet das Indexieren eines dichten Tages nichts extra und der Abruf funktioniert ohne Netz. Auf dein eigenes Gedächtnis läuft kein Zähler pro Token — deshalb müssen wir nie weniger von deinem Tag behalten, um das Produkt billiger zu machen.' },
      { title: 'Jede Erinnerung trägt ihre Quelle', body: 'Einträge verweisen auf das Ereignis, aus dem sie entstanden sind. Das erlaubt die Antwort „Dienstag 14:20, im Design-Dokument" statt einer selbstbewussten Vermutung — und hält unsichere Lesarten aus allem heraus, was du verschickst.' },
    ],
    steps: [
      { title: 'Einmal Bedienungshilfen freigeben', body: 'macOS fragt, du gibst frei, und die Erfassung folgt dem Fenster, in dem du tatsächlich arbeitest. Aus ausgeschlossenen Apps wird nichts gelesen.' },
      { title: 'Arbeite wie sonst', body: 'Kontext sammelt sich ohne Notizen. Der letzte Tag bleibt sofort verfügbar, der letzte Monat vollständig, alles Ältere komprimiert.' },
      { title: 'Nachsehen und ausdünnen', body: 'Löschst du einen Eintrag, verschwindet er aus Suche, Speicher und aus jedem Kontext, der später zusammengestellt wird. Pausieren geht jederzeit: Was nicht erfasst wurde, ist auch nicht gespeichert.' },
    ],
    outcomes: ['Wochen später die Begründung einer Entscheidung wiederfinden', 'Die Quelle einer Zahl finden, statt sie neu herzuleiten', 'Zu unterbrochener Arbeit zurückkehren, ohne die Geschichte neu zu bauen', 'Sensiblen Kontext standardmäßig auf dem Gerät behalten'],
    faq: [
      ['Macht ShogunAI Screenshots?', 'Nur dort, wo die Texterfassung nichts liefert, und nur wenn du Visual Recall einschaltest — standardmäßig ist es aus. Diese Screenshots bleiben verschlüsselt auf deinem Mac, du kannst sie jederzeit ansehen oder löschen, und sie verschwinden automatisch bei der gewählten Aufbewahrungsdauer. Überall sonst ist die Erfassung Text über die Bedienungshilfen, und es wird kein Bild geschrieben.'],
      ['Wird Meeting-Audio gespeichert?', 'Nein. Audio wird während des Meetings für das Transkript verarbeitet und nie auf die Festplatte oder in eine temporäre Datei geschrieben. Bestehen bleibt der Transkripttext und seine Herkunft.'],
      ['Wo liegt das Gedächtnis?', 'In einer verschlüsselten Datenbank auf deinem Mac. Es gibt keine serverseitige Kopie deines Tages, die man beschlagnahmen, abgreifen oder still zum Training nutzen könnte. Hinaus geht nur der Ausschnitt, den eine Anfrage braucht — mit seinem Zweck protokolliert.'],
      ['Kostet ein dichter Tag mehr?', 'Nein. Embeddings laufen lokal, das Indexieren ist unabhängig vom Volumen kostenlos, und der Abruf funktioniert offline.'],
    ],
  },
  {
    slug: 'contextual-recall',
    eyebrow: 'Kontextsuche',
    title: 'Frag nach deiner Woche, ohne die Vorgeschichte einzufügen',
    description: 'Eine Frage über Mail, Chat, Dokumente und Kalender, beantwortet aus deiner eigenen Historie — mit Quelle und Zeitpunkt an jedem Treffer.',
    intro: 'Ein allgemeiner Assistent startet jede Sitzung leer, also gehen die ersten Minuten dafür drauf, dein Leben neu zu erklären, bevor du etwas Nützliches fragen kannst. ShogunAI startet aus dem Gedächtnis, das es ohnehin gebaut hat — die Frage darf also die kurze sein, die du wirklich hattest.',
    highlights: [
      { title: 'Frag, wie du dich erinnerst', body: 'Nach Person, Projekt, Entscheidung oder ungefährem Zeitpunkt, nicht nur nach exaktem Stichwort. Die hybride Suche fährt Volltext und Vektoren gemeinsam, also landen sowohl eine halb erinnerte Formulierung als auch eine vage Beschreibung.' },
      { title: 'Eine Frage, alle verbundenen Werkzeuge', body: 'Mail, Chat, Dokumente und Kalender aus einer Suche statt aus vieren. Was du freigegeben hast, ist dabei; was nicht, bleibt draußen.' },
      { title: 'Antworten, die du prüfen kannst', body: 'Jeder Treffer trägt Quelle und Zeitpunkt. Du bekommst „Dienstag 14:20, im Design-Dokument" und kannst es öffnen, statt einem Absatz zu vertrauen, hinter dem nichts steht.' },
      { title: 'Konfidenz ist sichtbar', body: 'Jeder Zustand — Menschen, Projekte, Zusagen, offene Schleifen — trägt einen Konfidenzwert. Eine unsichere Lesart erscheint als Frage, nie als Tatsache in etwas, das du gerade abschicken willst.' },
    ],
    steps: [
      { title: 'Fragen', body: 'Beschreib in normaler Sprache, was du brauchst, auch wenn du dich nur an einen Teil erinnerst.' },
      { title: 'Abrufen', body: 'Die Suche geht in die warme Ebene — den letzten Monat in voller Tiefe — und liefert Treffer mit Herkunft zurück.' },
      { title: 'Weitermachen', body: 'Mach aus der Antwort ein Briefing, eine Antwort oder den nächsten Schritt, ohne den Kontext neu aufzubauen.' },
    ],
    outcomes: ['In ein Meeting gehen und wissen, wo es aufgehört hat', 'Nach zwei Wochen Abwesenheit eine Entscheidung wiederfinden', 'Aufhören, einen Thread in vier Apps zu suchen', 'Aus Fakten entwerfen statt aus dem Gedächtnis'],
    faq: [
      ['Wie unterscheidet sich das von einer Suche?', 'Suche erwartet die exakten Wörter und eine App nach der anderen. Der Abruf holt nach Bedeutung über die verbundenen Werkzeuge hinweg und hängt Quelle und Zeitpunkt an jedes Ergebnis, damit die Antwort überprüfbar ist.'],
      ['Funktioniert es offline?', 'Für den Abruf ja. Index und Suche laufen auf deinem Mac, Finden braucht also kein Netz. Die Generierung — eine Antwort entwerfen, eine schwierige Frage durchdenken — ist der Teil, der ein Modell braucht.'],
      ['Und wenn es unsicher ist?', 'Unsicherer Zustand wird abgeschwächt statt behauptet. Er kommt als „du hast Mika die korrigierten Zahlen vielleicht noch nicht geschickt" zurück, nicht als Satz, der still in einem Entwurf hart wird.'],
      ['In welchem Plan ist das?', 'In Standard. Erfassung, Abruf, Tagesbriefing und die lesenden Verbindungen der ersten Ebene stecken alle im kleineren Plan.'],
    ],
  },
  {
    slug: 'execution-layer',
    eyebrow: 'Ausführungsebene',
    title: 'Antworten ist die leichte Hälfte',
    description: 'Entwürfe, Updates und Aktionen in den Werkzeugen, die du schon nutzt — unter drei benannten Stufen der Autonomie und einem Halt, bevor etwas eine andere Person erreicht.',
    intro: 'Die meisten Gedächtnisprodukte antworten gut und lassen dir die Arbeit: Entwurf kopieren, Mail öffnen, Empfänger korrigieren, Datei suchen, anhängen, senden. Die Ausführungsebene schließt genau diese Lücke, und ihre Regeln sind sichtbar, bevor irgendetwas läuft.',
    highlights: [
      { title: 'Option drücken, und es schreibt an der Schreibmarke', body: 'Die Inline-Komposition liest das Feld um deinen Cursor und das Gedächtnis dahinter und schreibt die Fortsetzung direkt in die App, in der du ohnehin tippst. Ein gerätelokaler Schreibvorgang: Gesendet wird nichts, senden tust du selbst.' },
      { title: 'Drei Stufen, und die Linie verschiebt sich nicht', body: 'Stufe eins ist umkehrbar und lokal und läuft einfach. Stufe zwei wird entworfen und wartet auf dich. Stufe drei ist alles, was eine andere Person erreicht — Mail senden, eine Nachricht posten, einen Termin in fremde Kalender legen — und hält immer zur Freigabe an. Kein Prompt verschiebt eine Aktion zwischen den Stufen.' },
      { title: 'Dein Plan oder dein Schlüssel', body: 'Die Ausführung läuft mit dem Assistenz-Abo, das du ohnehin zahlst, innerhalb dessen Grenzen, oder mit einem eigenen API-Schlüssel. Du wählst den Anbieter und kannst wechseln, ohne einen Tag Gedächtnis zu verlieren. Schlüssel liegen im Systemschlüsselbund und sonst nirgends.' },
      { title: 'Ein Nachweis dessen, was lief', body: 'Jede Aktion hinterlässt, was lief, auf welcher Grundlage und was das Gerät verlassen hat — markiert, wenn es über einen Dritten ging. Dieses Protokoll macht Stufe eins vertretbar: Automatisierung, die man hinterher prüfen kann, ist ein anderes Angebot als eine, der man vorher glauben muss.' },
    ],
    steps: [
      { title: 'Verstehen', body: 'Die Anfrage wird gegen den Zustand deiner Arbeit gelesen — wer beteiligt ist, was entschieden wurde, was offen liegt — und nicht nur gegen den Thread.' },
      { title: 'Vorbereiten', body: 'Entwurf, Update oder Aktion entstehen mit der richtigen Datei bereits im Anhang und der offenen Frage bereits beantwortet.' },
      { title: 'Freigeben', body: 'Alles Folgenreiche wartet auf eine Freigabe. Umkehrbares ist längst fertig, wenn du hinsiehst.' },
    ],
    outcomes: ['Das Follow-up mit der richtigen Version im Anhang senden', 'Vorbereitet ins 15-Uhr-Meeting gehen', 'Die Schleife schließen, die du offen vergessen hattest', 'Den Senden-Knopf in der eigenen Hand behalten'],
    faq: [
      ['Kann es etwas senden, ohne zu fragen?', 'Nein. Alles, was eine andere Person erreicht, ist Stufe drei und hält zur Freigabe an. Das ist eine Eigenschaft davon, wie Aktionen geroutet werden, keine Einstellung, die du richtig treffen musst.'],
      ['Umgeht der API-Weg die Freigabe?', 'Nein. Derselbe Klassifikator und dieselben Freigaben gelten über MCP, CLI und REST. Ein Agent von außen hat nicht mehr Befugnis als dein eigener Klick.'],
      ['Brauche ich einen API-Schlüssel?', 'Nein. Es kann mit dem Assistenz-Abo laufen, das du ohnehin zahlst. Ein eigener Schlüssel ist die Alternative, nicht die Voraussetzung.'],
      ['In welchem Plan ist das?', 'In Pro, zusammen mit der Memory API und den Verbindungen der zweiten Ebene. Standard deckt Erfassung, Abruf und alltägliche Ausführung ab.'],
    ],
  },
];

const deUseCases: readonly MarketingDetail[] = [
  { slug: 'founders', eyebrow: 'Für Gründer', title: 'Bewahre den Unternehmenskontext, den du nicht verlieren darfst', description: 'Erinnere Entscheidungen, bereite Updates vor und verwandle verstreuten Unternehmenskontext in Handlungen.', intro: 'Gründer wechseln täglich zwischen Produkt, Recruiting, Kunden, Finanzierung und Betrieb. ShogunAI hält den Kontext hinter diesen Wechseln verfügbar.', highlights: [{ title: 'Investorenvorbereitung', body: 'Führe frühere Updates, Kennzahlendiskussionen, offene Fragen und Zusagen zusammen.' }, { title: 'Entscheidungshistorie', body: 'Finde heraus, warum das Team eine Richtung gewählt hat.' }, { title: 'Konsequentes Follow-up', body: 'Verwandle Gespräche in Entwürfe, Updates und klare nächste Schritte.' }], steps: [{ title: 'Betriebskontext erfassen', body: 'Baue Gedächtnis über Projekte, Gespräche und Recherche auf.' }, { title: 'Vor Entscheidungen fragen', body: 'Rufe Zusagen, Einwände und Annahmen ab.' }, { title: 'Follow-up abschließen', body: 'Bereite das Update aus demselben Kontext vor und gib es frei.' }], outcomes: ['Board- und Investorenupdates vorbereiten', 'Finanzierungsgespräche fortsetzen', 'Recruitingkontext bewahren', 'Kontextwechsel reduzieren'], faq: [['Ist ShogunAI eine Wissensdatenbank?', 'Es ist primär eine private Gedächtnis- und Ausführungsebene für Einzelne und ergänzt geteiltes Wissen.'], ['Hilft es bei Investorengesprächen?', 'Ja. Frühere Diskussionen, Fragen und Zusagen können ein fundiertes Briefing bilden.'], ['Muss das Team seine Tools wechseln?', 'Nein. ShogunAI arbeitet über bestehende Tools hinweg.']] },
  { slug: 'product-engineering', eyebrow: 'Für Produkt & Entwicklung', title: 'Trage Produktkontext von der Diskussion bis zur Auslieferung', description: 'Rufe technische Entscheidungen, Kundensignale, Designabwägungen und Projekthistorie toolübergreifend ab.', intro: 'Kontext geht an den Übergängen zwischen E-Mail, Slack, Figma und Linear verloren. ShogunAI verbindet diese Momente in einer persönlichen Kontextebene.', highlights: [{ title: 'Entscheidungen erinnern', body: 'Finde Gründe, Alternativen und Einschränkungen hinter einer Wahl.' }, { title: 'Schnellere Übergaben', body: 'Bereite kompakten Kontext vor, ohne jede Quelle neu zusammenzusetzen.' }, { title: 'Projektkontinuität', body: 'Kehre mit Verlauf und nächsten Schritten zu unterbrochener Arbeit zurück.' }], steps: [{ title: 'Arbeit begleiten', body: 'Erfasse Kontext zwischen Recherche, Design und Umsetzung.' }, { title: 'Das Warum abrufen', body: 'Frage nach Feature, Fehler, Kunde oder Entscheidung.' }, { title: 'Artefakt erstellen', body: 'Verwandle Kontext in Briefing, Issue oder Update.' }], outcomes: ['Bessere Produktbriefings schreiben', 'Technische Gründe wiederfinden', 'Sprintupdates vorbereiten', 'Alte Projekte schneller aufnehmen'], faq: [['Ersetzt es Linear, Jira oder Notion?', 'Nein. Es ist eine Kontext- und Ausführungsebene über bestehenden Tools.'], ['Verbindet es Code und Produktkontext?', 'Es verknüpft Diskussionen und Artefakte rund um die Umsetzung in autorisierten Tools.'], ['Ist es für Individual Contributors geeignet?', 'Ja. Es ist auf das private Arbeitsgedächtnis von Entwicklern, Designern und PMs ausgelegt.']] },
  { slug: 'consultants', eyebrow: 'Für Berater', title: 'Erinnere jeden Kunden, ohne alles im Kopf zu behalten', description: 'Bewahre kundenspezifischen Kontext, bereite dich schneller vor und erstelle Follow-ups auf Basis geleisteter Arbeit.', intro: 'Beratung verlangt schnelle Wechsel zwischen Unternehmen, Personen, Begriffen und Zusagen. ShogunAI stellt vor dem Gespräch den richtigen Kontext bereit und unterstützt danach die Ausarbeitung.', highlights: [{ title: 'Kundenerinnerung', body: 'Rufe Gespräche, Einschränkungen, Ergebnisse und offene Fragen pro Projekt ab.' }, { title: 'Meetingvorbereitung', body: 'Erstelle ein Briefing ohne jede Nachricht und jedes Dokument zu prüfen.' }, { title: 'Konsistentes Follow-up', body: 'Entwirf Zusammenfassungen und nächste Schritte auf Basis des Gesprächs.' }], steps: [{ title: 'Kundengedächtnis aufbauen', body: 'Erfasse nötigen Kontext local-first.' }, { title: 'Vor dem Termin vorbereiten', body: 'Rufe Änderungen, Zusagen und offene Entscheidungen ab.' }, { title: 'Danach liefern', body: 'Erstelle Zusammenfassung oder Plan und prüfe ihn vor dem Versand.' }], outcomes: ['Mit weniger Aufwand Kunden wechseln', 'Bessere Briefings erstellen', 'Vergessene Zusagen reduzieren', 'Angebote schneller entwerfen'], faq: [['Kann ich Kundenkontexte trennen?', 'Du kontrollierst, was erfasst und welche Dienste verbunden werden.'], ['Werden Kundenmails automatisch gesendet?', 'Kundenbezogene Aktionen benötigen vor dem Versand deine Freigabe.'], ['Liegt das Gedächtnis nur in der Cloud?', 'Nein. Es ist standardmäßig local-first und teilt Kontext nur für autorisierte Funktionen.']] },
];

export const localizedMarketingContent = {
  ja: { features: jaFeatures, useCases: jaUseCases },
  es: { features: esFeatures, useCases: esUseCases },
  de: { features: deFeatures, useCases: deUseCases },
} as const;
