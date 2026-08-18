import type { MarketingDetail } from './marketing-content';

const jaFeatures: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory', eyebrow: 'AIメモリ', title: '仕事の裏側にある文脈まで残す、プライベートな記憶',
    description: 'ShogunAIはMac上の仕事の文脈を取得し、スクリーンショットに頼らない、検索可能で構造化されたタイムラインに整理します。',
    intro: '仕事で本当に重要な情報は、一つの文書には収まりません。会話、ブラウザのタブ、意思決定、書きかけの下書きに分散しています。ShogunAIはそれらを後からたどれる記憶として残し、毎回ゼロから経緯を組み立て直す手間を減らします。',
    highlights: [
      { title: 'ローカルファースト', body: '仕事の記憶は、既定では端末内に保存されます。外部プロバイダへ必要な文脈を渡すのは、利用者が機能を有効にした場合だけです。' },
      { title: '画像ではなく文脈', body: '大量のスクリーンショットを見返す方式ではなく、検索・再利用できる構造化された仕事の文脈を中心に設計しています。任意機能のビジュアルリコールだけが狭い例外で、オプトイン・ローカル保存・72時間で失効します。' },
      { title: '受動的に記録', body: 'すべてを手作業でメモに変えなくても、普段どおりに働きながら役立つ記憶を蓄積できます。' },
    ],
    steps: [
      { title: '取得する', body: '許可した範囲の仕事の文脈をmacOS上で取得し、端末内に整理します。' },
      { title: 'つなげる', body: '出来事、人、プロジェクト、ツールにまたがる情報を、一つの検索可能な履歴として関連付けます。' },
      { title: '管理する', body: '取得の一時停止、連携サービスの選択、不要なローカル記憶の削除を自分で管理できます。' },
    ],
    outcomes: ['意思決定の理由まで思い出せる', '情報の根拠をすぐに見つけられる', '中断した仕事へ早く戻れる', '機密性の高い文脈を既定で端末内に保てる'],
    faq: [
      ['ShogunAIはスクリーンショットを保存しますか？', '既定では保存しません。キャプチャはmacOSのアクセシビリティ経由のテキストのみで、画像は書き込みません。唯一の例外が「ビジュアルリコール」で、これは利用者が自分で有効にする機能です。有効かつウィンドウからテキストが一切取得できなかった場合にかぎり、圧縮したフレームを暗号化されたローカルDBに最大72時間だけ保持し、期限が来ると自動削除します。フレームがMacの外へ出ることはなく、いずれの場合もスクリーンショットの保管庫は作りません。'],
      ['記憶はどこに保存されますか？', '記憶はローカルファーストで、既定ではMac内に保存されます。任意の連携機能を使う場合のみ、承認した処理に必要な文脈が外部サービスへ送られることがあります。'],
      ['手作業でメモを書く必要がありますか？', '必須ではありません。受動的な取得によって手作業を減らしつつ、必要に応じて文脈を追加・削除できます。'],
    ],
  },
  {
    slug: 'contextual-recall', eyebrow: '文脈検索', title: '前提を貼り付けなくても、自分の仕事について質問できる',
    description: '意思決定、会話、文書、プロジェクトの経緯を自然な言葉で検索し、自分の仕事に根ざした回答を得られます。',
    intro: '一般的なチャットAIは、会話を始めるたびに前提が空の状態です。ShogunAIは、すでに蓄積された仕事の文脈から始めます。複数のアプリを検索し、起きたことを自分で再構成する負担を減らします。',
    highlights: [
      { title: '自然言語で検索', body: '正確なキーワードだけでなく、人、プロジェクト、意思決定、時期、目的など、覚えている手がかりから質問できます。' },
      { title: 'ツールを横断する文脈', body: 'メッセージ、メール、文書、ブラウザ調査に分散した関連情報を、一つの流れとして扱えます。' },
      { title: '自分の履歴に基づく回答', body: '一般論ではなく、自分の仕事で実際に起きたことを出発点に回答を組み立てます。' },
    ],
    steps: [
      { title: '質問する', body: '一部しか覚えていなくても、普段の言葉で知りたいことを伝えます。' },
      { title: '取り出す', body: 'プライベートな仕事の記憶から、質問に最も関係する文脈を見つけます。' },
      { title: '次へ進める', body: '得られた文脈を、要約、返信、計画、次のアクションへそのままつなげます。' },
    ],
    outcomes: ['Slack・Gmail・文書を探し回る時間を減らす', '会議前に必要な経緯をまとめる', '数週間前の判断を根拠ごと取り戻す', '既知の事実から下書きを作る'],
    faq: [
      ['通常の検索とは何が違いますか？', '通常の検索は正確な語句やアプリごとの操作が必要です。文脈検索は、意味、人、プロジェクト、意思決定の関係から情報を取り出します。'],
      ['複数のツールをまたいで質問できますか？', 'はい。許可した連携ツールとローカルの仕事の記憶を組み合わせ、ワークフロー全体を一つの文脈レイヤーとして扱う設計です。'],
      ['利用するAIプロバイダを選べますか？', 'はい。BYOKに対応し、サポート対象のAIプロバイダと鍵を自分で選べます。'],
    ],
  },
  {
    slug: 'execution-layer', eyebrow: '実行レイヤー', title: '記憶した文脈を、完了した仕事へ変える',
    description: '連携ツールを横断して、検索から実行まで進めます。重要な操作は承認を挟み、最終的な判断を利用者に残します。',
    intro: '記憶は、実際の作業を減らしてこそ価値があります。ShogunAIは依頼の背景を理解し、返信の下書き、文書の整理、会議準備などの次の工程を用意します。送信や変更など重要な操作は、実行前に確認できます。',
    highlights: [
      { title: '文脈を踏まえた下書き', body: '関連するプロジェクト履歴、判断、好みを最初から反映した状態で作業を始められます。' },
      { title: '20以上のツールと連携', body: '新しい作業場所へすべてを移すのではなく、現在利用しているツールを横断して処理します。' },
      { title: '重要操作の承認', body: '送信、公開、変更など影響のある操作は、完了前に内容を確認できる設計です。' },
    ],
    steps: [
      { title: '理解する', body: '依頼を正確に解釈するために必要な文脈を取り出します。' },
      { title: '準備する', body: '実行レイヤーが下書き、計画、更新内容、ツール操作を準備します。' },
      { title: '承認する', body: '影響のある操作は利用者が確認し、承認後に完了します。' },
    ],
    outcomes: ['会議後のフォローを下書きする', 'プロジェクトや投資家向け更新を準備する', '文書を一貫したルールで整理する', '質問から次の作業完了までつなげる'],
    faq: [
      ['承認なしで操作されることはありますか？', '送信・公開・変更など重要な操作は承認を挟む設計です。最終的に何を実行するかは利用者が管理します。'],
      ['どのような作業を任せられますか？', '返信の下書き、会議準備、文書整理、ブリーフ作成、連携ツールへの文脈反映などを想定しています。'],
      ['実行機能はどのプランに含まれますか？', 'Standardには主要ツールとの日々の実行が含まれます。Proでは無制限のメモリと検索、全ツール連携、自律実行を利用できます。'],
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
    slug: 'ai-memory', eyebrow: 'Memoria de IA', title: 'Una memoria privada para el contexto detrás de tu trabajo',
    description: 'ShogunAI captura el contexto de trabajo en tu Mac y lo convierte en una línea de tiempo estructurada y consultable, sin depender de capturas de pantalla.',
    intro: 'La información importante rara vez vive en un solo documento. Está repartida entre conversaciones, pestañas, decisiones y borradores. ShogunAI conserva ese contexto para que puedas retomarlo sin reconstruir toda la historia.',
    highlights: [{ title: 'Local-first por defecto', body: 'La memoria de trabajo permanece en tu dispositivo por defecto. Tú decides cuándo un proveedor conectado recibe contexto relevante.' }, { title: 'Contexto, no capturas', body: 'Está diseñado alrededor de contexto estructurado y consultable, no de un archivo de imágenes que debas revisar. El recuerdo visual opcional es la excepción estrecha: se activa por elección, se cifra localmente y caduca tras el plazo finito que eliges.' }, { title: 'Captura pasiva', body: 'Crea una memoria útil mientras trabajas, sin convertir cada decisión en una tarea manual de notas.' }],
    steps: [{ title: 'Capturar', body: 'Organiza localmente el contexto de trabajo que autorizas en macOS.' }, { title: 'Conectar', body: 'Relaciona momentos, personas, proyectos y herramientas en un historial consultable.' }, { title: 'Controlar', body: 'Pausa la captura, elige servicios conectados y elimina memoria local cuando lo necesites.' }],
    outcomes: ['Recordar por qué se tomó una decisión', 'Encontrar la fuente de un dato', 'Retomar antes el trabajo interrumpido', 'Mantener contexto sensible en local por defecto'],
    faq: [['¿ShogunAI guarda capturas de pantalla?', 'No por defecto: la captura lee texto mediante la capa de accesibilidad de macOS y no escribe ninguna imagen. La única excepción es el recuerdo visual, que activas tú. Con él activo, y solo cuando una ventana no ofrece texto alguno, se guarda un fotograma comprimido en la base de datos cifrada local durante el plazo finito que eliges y luego se borra solo. Los fotogramas nunca salen de tu Mac.'], ['¿Dónde se guarda la memoria?', 'La memoria es local-first y permanece en tu Mac por defecto. Los servicios opcionales solo reciben el contexto necesario para una acción autorizada.'], ['¿Tengo que tomar notas manualmente?', 'No. La captura pasiva reduce el trabajo manual y permite añadir o eliminar contexto de forma intencional.']],
  },
  {
    slug: 'contextual-recall', eyebrow: 'Recuperación contextual', title: 'Pregunta por tu trabajo sin pegar toda la historia',
    description: 'Recupera decisiones, conversaciones, documentos y contexto de proyectos con lenguaje natural y respuestas basadas en tu propio historial.',
    intro: 'Los chatbots generales empiezan sin contexto. ShogunAI comienza con la memoria de trabajo que ya has creado y reduce la necesidad de buscar en varias aplicaciones y reconstruir lo ocurrido.',
    highlights: [{ title: 'Búsqueda en lenguaje natural', body: 'Pregunta por persona, proyecto, decisión, fecha o intención, no solo por una palabra exacta.' }, { title: 'Contexto entre herramientas', body: 'Relaciona información dispersa entre mensajes, correo, documentos e investigación web.' }, { title: 'Respuestas fundamentadas', body: 'Parte de tu propio historial de trabajo en lugar de ofrecer una respuesta genérica.' }],
    steps: [{ title: 'Preguntar', body: 'Describe lo que necesitas con palabras normales, aunque solo recuerdes una parte.' }, { title: 'Recuperar', body: 'ShogunAI identifica el contexto más relevante de tu memoria privada.' }, { title: 'Continuar', body: 'Convierte la respuesta en un resumen, correo, plan o siguiente acción.' }],
    outcomes: ['Buscar menos en Slack, Gmail y documentos', 'Preparar reuniones con el contexto completo', 'Recuperar decisiones semanas después', 'Redactar a partir de hechos conocidos'],
    faq: [['¿En qué se diferencia de una búsqueda normal?', 'La búsqueda normal exige palabras exactas y búsquedas separadas. La recuperación contextual usa significado, personas, proyectos y decisiones.'], ['¿Puede responder entre varias herramientas?', 'Sí. Combina la memoria local con las herramientas que autorizas para crear una capa de contexto común.'], ['¿Puedo elegir proveedor de IA?', 'Sí. ShogunAI admite BYOK para que elijas un proveedor compatible y gestiones tus claves.']],
  },
  {
    slug: 'execution-layer', eyebrow: 'Capa de ejecución', title: 'Convierte el contexto recordado en trabajo terminado',
    description: 'Pasa de la recuperación a la acción entre herramientas conectadas, con aprobación para las operaciones importantes.',
    intro: 'La memoria solo es útil cuando reduce trabajo. ShogunAI usa el contexto de una petición para preparar la siguiente acción y mantiene bajo tu control cualquier envío, publicación o cambio relevante.',
    highlights: [{ title: 'Borradores con contexto', body: 'Empieza con el historial, las decisiones y las preferencias relevantes ya incorporadas.' }, { title: 'Más de 20 herramientas', body: 'Trabaja entre las herramientas que ya utilizas sin migrar todo a otro espacio.' }, { title: 'Aprobación humana', body: 'Revisa acciones que envían, publican o modifican información antes de completarlas.' }],
    steps: [{ title: 'Comprender', body: 'Recupera el contexto necesario para interpretar la solicitud.' }, { title: 'Preparar', body: 'Crea el borrador, plan, actualización o acción de herramienta.' }, { title: 'Aprobar', body: 'Revisas las acciones importantes antes de que se completen.' }],
    outcomes: ['Redactar seguimientos de reuniones', 'Preparar actualizaciones de proyectos', 'Organizar documentos con consistencia', 'Pasar de una pregunta a la siguiente acción'],
    faq: [['¿Puede actuar sin mi aprobación?', 'Las acciones importantes usan controles de aprobación. Tú decides qué se envía, cambia o publica.'], ['¿Con qué tareas ayuda?', 'Puede preparar respuestas, reuniones, documentos, resúmenes y acciones en herramientas conectadas.'], ['¿Qué plan incluye ejecución?', 'Standard incluye ejecución diaria con las conexiones principales. Pro añade memoria y recuperación ilimitadas, todas las herramientas y ejecución autónoma.']],
  },
];

const esUseCases: readonly MarketingDetail[] = [
  { slug: 'founders', eyebrow: 'Para fundadores', title: 'Conserva el contexto de la empresa que no puedes perder', description: 'Recuerda decisiones, prepara actualizaciones para inversores y equipos, y convierte contexto disperso en acción.', intro: 'Un fundador cambia constantemente entre producto, contratación, clientes, financiación y operaciones. ShogunAI mantiene disponible el contexto de esos cambios.', highlights: [{ title: 'Preparación para inversores', body: 'Reúne informes anteriores, métricas, preguntas abiertas y compromisos.' }, { title: 'Historial de decisiones', body: 'Recupera por qué el equipo eligió una dirección, no solo el resultado final.' }, { title: 'Seguimiento', body: 'Convierte reuniones en borradores, actualizaciones y siguientes pasos claros.' }], steps: [{ title: 'Captura el contexto operativo', body: 'Crea memoria entre proyectos, conversaciones e investigación.' }, { title: 'Pregunta antes de decidir', body: 'Recupera compromisos, objeciones y supuestos.' }, { title: 'Completa el seguimiento', body: 'Prepara y aprueba la actualización desde el mismo contexto.' }], outcomes: ['Preparar actualizaciones para inversores', 'Retomar conversaciones de financiación', 'Mantener contexto de contratación', 'Reducir cambios mentales de contexto'], faq: [['¿Es una base de conocimiento empresarial?', 'Es una memoria y capa de ejecución privada para el individuo que complementa el conocimiento compartido.'], ['¿Ayuda con reuniones de inversores?', 'Sí. Recupera conversaciones, preguntas y compromisos para preparar un briefing fundamentado.'], ['¿El equipo debe cambiar de herramientas?', 'No. Está diseñado para trabajar entre las herramientas existentes.']] },
  { slug: 'product-engineering', eyebrow: 'Para producto e ingeniería', title: 'Lleva el contexto de producto desde la discusión hasta la entrega', description: 'Recupera decisiones técnicas, evidencia de clientes, compromisos de diseño e historial del proyecto.', intro: 'El contexto se pierde entre correo, Slack, Figma y Linear. ShogunAI crea una capa personal que conecta esos momentos.', highlights: [{ title: 'Recuerdo de decisiones', body: 'Encuentra razones, alternativas y restricciones detrás de una elección.' }, { title: 'Traspasos más rápidos', body: 'Prepara contexto claro sin reconstruir manualmente cada fuente.' }, { title: 'Continuidad del proyecto', body: 'Vuelve al trabajo interrumpido con historial y siguientes pasos.' }], steps: [{ title: 'Seguir el trabajo', body: 'Captura contexto entre investigación, diseño e implementación.' }, { title: 'Recuperar el porqué', body: 'Pregunta por una función, error, cliente o decisión.' }, { title: 'Crear el artefacto', body: 'Convierte contexto en un brief, incidencia o actualización.' }], outcomes: ['Escribir mejores briefs', 'Recuperar razones técnicas', 'Preparar actualizaciones de sprint', 'Volver rápido a proyectos antiguos'], faq: [['¿Sustituye a Linear, Jira o Notion?', 'No. Es una capa de contexto y ejecución sobre tus herramientas actuales.'], ['¿Conecta código y contexto de producto?', 'Está diseñado para relacionar discusiones y artefactos alrededor de la implementación.'], ['¿Sirve a colaboradores individuales?', 'Sí. Está centrado en la memoria privada de ingenieros, diseñadores y PM.']] },
  { slug: 'consultants', eyebrow: 'Para consultores', title: 'Recuerda a cada cliente sin guardar todo en tu cabeza', description: 'Mantén contexto por cliente, prepárate más rápido y crea seguimientos basados en el trabajo realizado.', intro: 'La consultoría exige cambiar entre empresas, personas, términos y compromisos. ShogunAI recupera el contexto correcto antes de una llamada y ayuda a entregar después.', highlights: [{ title: 'Recuerdo del cliente', body: 'Recupera conversaciones, restricciones, entregables y preguntas por proyecto.' }, { title: 'Preparación de reuniones', body: 'Crea un briefing sin revisar cada mensaje y documento.' }, { title: 'Seguimiento consistente', body: 'Redacta resúmenes y próximos pasos basados en lo hablado.' }], steps: [{ title: 'Crear memoria de cliente', body: 'Captura el contexto necesario con un enfoque local-first.' }, { title: 'Preparar la llamada', body: 'Recupera cambios, compromisos y decisiones abiertas.' }, { title: 'Entregar después', body: 'Crea un resumen o plan y revísalo antes de enviarlo.' }], outcomes: ['Cambiar de cliente con menos carga', 'Crear mejores briefings', 'Reducir compromisos olvidados', 'Redactar propuestas más rápido'], faq: [['¿Puedo separar contextos de clientes?', 'Tú controlas qué se captura y qué servicios se conectan.'], ['¿Envía correos automáticamente?', 'Las acciones dirigidas a clientes requieren aprobación antes de enviarse.'], ['¿La memoria está solo en la nube?', 'No. Es local-first por defecto y solo comparte contexto cuando una función autorizada lo necesita.']] },
];

const deFeatures: readonly MarketingDetail[] = [
  { slug: 'ai-memory', eyebrow: 'KI-Gedächtnis', title: 'Ein privates Gedächtnis für den Kontext hinter deiner Arbeit', description: 'ShogunAI erfasst Arbeitskontext auf deinem Mac und macht ihn als strukturierte, durchsuchbare Zeitleiste nutzbar – ohne Screenshot-Archiv.', intro: 'Wichtige Informationen liegen selten in einem Dokument. Sie verteilen sich auf Gespräche, Tabs, Entscheidungen und Entwürfe. ShogunAI bewahrt diesen Kontext, damit du die Geschichte nicht jedes Mal neu zusammensetzen musst.', highlights: [{ title: 'Local-first als Standard', body: 'Dein Arbeitsgedächtnis bleibt standardmäßig auf deinem Gerät. Du entscheidest, wann ein verbundener Anbieter relevanten Kontext erhält.' }, { title: 'Kontext statt Screenshots', body: 'Im Mittelpunkt steht strukturierter, durchsuchbarer Arbeitskontext statt eines Bildarchivs. Die optionale visuelle Erinnerung ist die schmale Ausnahme: aktiv nur nach Zustimmung, lokal, nach 72 Stunden gelöscht.' }, { title: 'Passive Erfassung', body: 'Baue während der Arbeit ein nützliches Gedächtnis auf, ohne jede Entscheidung manuell zu notieren.' }], steps: [{ title: 'Erfassen', body: 'ShogunAI organisiert den von dir erlaubten Kontext lokal unter macOS.' }, { title: 'Verbinden', body: 'Momente, Personen, Projekte und Tools werden zu einem durchsuchbaren Verlauf.' }, { title: 'Kontrollieren', body: 'Pausiere die Erfassung, wähle Dienste und lösche lokale Erinnerungen.' }], outcomes: ['Entscheidungsgründe wiederfinden', 'Quellen schneller finden', 'Unterbrochene Arbeit fortsetzen', 'Sensiblen Kontext lokal halten'], faq: [['Speichert ShogunAI Screenshots?', 'Standardmäßig nicht: Die Erfassung liest Text über die Accessibility-Ebene von macOS und schreibt kein Bild. Die einzige Ausnahme ist die visuelle Erinnerung, die du selbst einschaltest. Ist sie aktiv und liefert ein Fenster überhaupt keinen Text, wird ein komprimiertes Einzelbild höchstens 72 Stunden in der verschlüsselten lokalen Datenbank gehalten und danach automatisch gelöscht. Einzelbilder verlassen deinen Mac nie, und ein Screenshot-Archiv entsteht in keinem Fall.'], ['Wo wird die Erinnerung gespeichert?', 'Das Gedächtnis ist local-first und bleibt standardmäßig auf deinem Mac. Optionale Dienste erhalten nur den für eine freigegebene Aktion nötigen Kontext.'], ['Muss ich manuell Notizen schreiben?', 'Nein. Passive Erfassung reduziert manuelle Notizen; Kontext kann bewusst ergänzt oder entfernt werden.']] },
  { slug: 'contextual-recall', eyebrow: 'Kontextsuche', title: 'Frage nach deiner Arbeit, ohne die Vorgeschichte einzufügen', description: 'Rufe Entscheidungen, Gespräche, Dokumente und Projektkontext in natürlicher Sprache aus deinem eigenen Arbeitsverlauf ab.', intro: 'Allgemeine Chatbots starten ohne Kontext. ShogunAI beginnt mit deinem bereits aufgebauten Arbeitsgedächtnis und reduziert die Suche über mehrere Apps.', highlights: [{ title: 'Natürliche Sprache', body: 'Suche nach Person, Projekt, Entscheidung, Zeit oder Absicht statt nur nach exakten Begriffen.' }, { title: 'Toolübergreifender Kontext', body: 'Verbinde Informationen aus Nachrichten, E-Mails, Dokumenten und Webrecherche.' }, { title: 'Fundierte Antworten', body: 'Dein eigener Arbeitsverlauf ist der Ausgangspunkt statt einer allgemeinen Antwort.' }], steps: [{ title: 'Fragen', body: 'Beschreibe dein Anliegen in normalen Worten, auch wenn du nur einen Teil erinnerst.' }, { title: 'Abrufen', body: 'ShogunAI findet den relevantesten Kontext in deinem privaten Gedächtnis.' }, { title: 'Weiterarbeiten', body: 'Verwandle die Antwort in Briefing, Nachricht, Plan oder Aktion.' }], outcomes: ['Weniger in Slack, Gmail und Dokumenten suchen', 'Meetings mit Kontext vorbereiten', 'Alte Entscheidungen nachvollziehen', 'Entwürfe aus bekannten Fakten erstellen'], faq: [['Was ist anders als bei normaler Suche?', 'Kontextsuche nutzt Bedeutung, Personen, Projekte und Entscheidungen statt nur exakter Wörter.'], ['Funktioniert sie über mehrere Tools?', 'Ja. Lokale Erinnerung und autorisierte Tools bilden eine gemeinsame Kontextebene.'], ['Kann ich den KI-Anbieter wählen?', 'Ja. Mit BYOK wählst du einen unterstützten Anbieter und verwaltest deine Schlüssel.']] },
  { slug: 'execution-layer', eyebrow: 'Ausführungsebene', title: 'Verwandle erinnerten Kontext in erledigte Arbeit', description: 'Gehe über verbundene Tools vom Abruf zur Aktion – mit Freigaben für folgenreiche Schritte.', intro: 'Erinnerung ist wertvoll, wenn sie Arbeit reduziert. ShogunAI nutzt den Kontext einer Anfrage, bereitet den nächsten Schritt vor und lässt wichtige Aktionen unter deiner Kontrolle.', highlights: [{ title: 'Kontextbezogene Entwürfe', body: 'Starte mit relevanter Projekthistorie, Entscheidungen und Präferenzen.' }, { title: 'Über 20 verbundene Tools', body: 'Arbeite in deinen bestehenden Tools statt alles in einen neuen Arbeitsbereich zu verschieben.' }, { title: 'Freigaben', body: 'Prüfe Aktionen zum Senden, Veröffentlichen oder Ändern vor der Ausführung.' }], steps: [{ title: 'Verstehen', body: 'Der nötige Kontext wird zur korrekten Interpretation abgerufen.' }, { title: 'Vorbereiten', body: 'Die Ausführungsebene erstellt Entwurf, Plan, Update oder Tool-Aktion.' }, { title: 'Freigeben', body: 'Du prüfst wichtige Aktionen, bevor sie abgeschlossen werden.' }], outcomes: ['Follow-ups nach Meetings entwerfen', 'Projektupdates vorbereiten', 'Dokumente konsistent organisieren', 'Von der Frage zur nächsten Aktion gelangen'], faq: [['Kann ShogunAI ohne Freigabe handeln?', 'Folgenreiche Aktionen verwenden Freigaben. Du bestimmst, was gesendet, geändert oder veröffentlicht wird.'], ['Bei welchen Aufgaben hilft es?', 'Zum Beispiel bei Antworten, Meetingvorbereitung, Dokumentorganisation, Briefings und Aktionen in verbundenen Tools.'], ['Welcher Plan enthält Ausführung?', 'Standard enthält tägliche Ausführung mit den wichtigsten Tool-Verbindungen. Pro ergänzt unbegrenztes Gedächtnis und Abruf, alle Tools und autonome Ausführung.']] },
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
