import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import matter from 'gray-matter';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import * as runtime from 'react/jsx-runtime';

const require = createRequire(import.meta.url);
const nextMdxRemoteDir = dirname(require.resolve('next-mdx-remote/package.json'));
const mdxModulePath = require.resolve('@mdx-js/mdx', { paths: [nextMdxRemoteDir] });
const { evaluate } = await import(pathToFileURL(mdxModulePath).href);

const BLOG_DIR = join(process.cwd(), 'content', 'blog');
const OUTPUT_FILE = join(process.cwd(), 'src', 'lib', 'blog-data.generated.ts');
const LOCALES = new Set(['en', 'ja', 'es', 'de']);
const CATEGORY_IMAGES = {
  'AI Memory': '/images/blog/ai-memory.png',
  'Work Context': '/images/blog/work-context.png',
  Comparisons: '/images/blog/comparisons.png',
  Privacy: '/images/blog/privacy.png',
  Product: '/images/blog/product.png',
};

// Every published article gets a real body for every supported locale. These
// editorial summaries are intentionally source-controlled so a language
// switch never silently renders English content under a translated URL.
const LOCALIZED_ARTICLE_COPY = {
  ja: {
    'ai-memory-context-layer-guide': ['AIに記憶が必要な理由', 'AIメモリとコンテキストレイヤー、プライバシー、実行までを実務目線で解説します。', ['AIモデルが賢くなっても、仕事の背景を毎回説明し直す必要があれば実務では十分に役立ちません。メモリは過去の決定や好みを保持し、コンテキストは今のタスクに必要な情報を組み立てます。', '有用なレイヤーには、選択的な取得、検索可能な保存、根拠の提示、明示的な権限、削除と一時停止の操作が必要です。', 'ShogunAIはローカルファーストの記憶と、承認を挟んだ実行を一つの個人向けレイヤーとして設計しています。']],
    'best-ai-memory-tools-for-knowledge-workers': ['ナレッジワーカー向けAIメモリーツールの選び方', '日々の決定、会話、資料を取り戻すAIメモリーツールを実務目線で比較します。', ['AIメモリーツールは、アプリや作業セッションの間で消える文脈を取り戻すための道具です。', '取得範囲を選べること、横断検索、根拠の確認、承認後の実行を重視して選びます。', 'ShogunAIは個人の仕事を中心に、ツールに散らばる文脈をローカル優先で扱います。']],
    'how-to-search-slack-gmail-notion': ['Slack・Gmail・Notionを横断して検索する方法', '分散した決定と文脈を実務で見つけるためのワークフローです。', ['仕事の文脈は一か所に保存されません。決定はSlackで始まり、Gmailで補足され、Notionにまとまることがあります。', 'アプリ名ではなく、決定、担当、次の行動を質問にすると、関連情報を時系列で再構成できます。', '検索結果には出典を付け、送信や変更は承認を挟みます。']],
    'investors-on-the-memory-and-context-layer': ['メモリとコンテキストレイヤーをめぐる投資家の見方', 'AIエージェントにおける文脈、記憶、同僚のような実行を一次情報に基づいて整理します。', ['AIの競争はモデルの大きさだけでなく、エージェントが仕事の背景を理解できるかへ移っています。', '文脈レイヤーはデータ、関係性、権限、暗黙知を扱い、メモリレイヤーは必要な情報を時間をまたいで保持します。', 'ShogunAIは、日々の記憶と承認された実行を個人の仕事に接続する位置にあります。']],
    'passive-memory-not-screenshots': ['スクリーンショットではなく、受動的な記憶', '監視フィードにせず、仕事の文脈をプライベートに取り戻す方法を解説します。', ['スクリーンショットは文脈の粗い代理です。何が重要だったか、何が変わったか、次に何をすべきかまでは説明しません。', '有用な受動的記憶は、端末内を基本に、検索、削除、一時停止、権限の境界を明確にします。']],
    'shogunai-vs-glean': ['ShogunAIとGleanの違い', '個人の仕事メモリーと企業向け検索の違いを整理します。', ['Gleanは組織の知識発見を中心にし、ShogunAIは個人の一日の仕事の文脈を中心にします。', '組織全体の検索基盤が必要か、個人の連続した仕事を取り戻したいかで選択が変わります。']],
    'shogunai-vs-mem': ['ShogunAIとMemの違い', 'AIメモリーレイヤーと個人ノートの役割を仕事の流れから比較します。', ['Memは自分で書いたノートの保存と想起が中心です。ShogunAIは会話、メール、資料、決定の周囲にある文脈を扱います。', '意図的に整理した知識にはノート、書く前に消えてしまう仕事の背景にはメモリーレイヤーが向いています。']],
    'shogunai-vs-notion': ['ShogunAIとNotionの違い', 'メモリーレイヤーとワークスペースの役割を整理します。', ['Notionは書く、整理する、共有するためのワークスペースです。ShogunAIは複数ツールに散らばる背景を取り出し、次の行動へつなげます。', '両者は置き換えではなく、正式な知識と日々の文脈を分担できます。']],
    'the-execution-layer': ['実行レイヤーとは何か', '記憶した文脈を、承認可能な仕事の次の一歩へ変える仕組みです。', ['メモリは、次の仕事を減らして初めて価値になります。ShogunAIは決定や好みを取得し、下書き、計画、更新、ツール操作を準備します。', '送信、公開、変更、削除などの重要操作は、根拠を確認してから承認できるようにします。']],
    'welcome-to-shogunai': ['ShogunAIへようこそ', '一日の記憶と、それに基づく実行を一つの個人向けOSとして考える理由です。', ['仕事の価値ある文脈は、一つの文書ではなく会話、ブラウザ、決定、途中の下書きに分散しています。', 'ShogunAIはその背景を取り戻し、次の一歩を準備します。ローカル優先、明示的なプロバイダ選択、重要操作の承認を原則にします。']],
    'why-local-first': ['なぜローカルファーストなのか', '仕事の記憶を自分の端末に置く意味と、そこから生まれる選択肢を説明します。', ['個人のメモリには、決定、人間関係、資料、未完成の考えが含まれます。必要のないクラウドコピーを作らないことが重要です。', '端末内の保存を基本にすると、削除が意味を持ち、AIプロバイダへ共有するタイミングを自分で選べます。']],
  },
  es: {
    'ai-memory-context-layer-guide': ['Por qué la IA necesita memoria', 'Una guía práctica sobre memoria, contexto, privacidad y ejecución para el trabajo diario.', ['Un modelo inteligente sigue empezando de cero si no conoce el contexto de tu trabajo. La memoria conserva decisiones y preferencias; el contexto reúne lo necesario para la tarea actual.', 'Una capa útil necesita captura selectiva, búsqueda con fuentes, permisos claros y controles de pausa y borrado.', 'ShogunAI une memoria local-first y ejecución con aprobación para el individuo.']],
    'best-ai-memory-tools-for-knowledge-workers': ['Cómo elegir herramientas de memoria de IA', 'Una guía práctica para recuperar decisiones, conversaciones y documentos del trabajo diario.', ['Las mejores herramientas no solo guardan notas: recuperan el contexto que desaparece entre aplicaciones.', 'Busca límites de captura, búsqueda transversal, fuentes verificables y aprobación antes de actuar.', 'ShogunAI trata el contexto personal como una capa privada y local-first.']],
    'how-to-search-slack-gmail-notion': ['Cómo buscar en Slack, Gmail y Notion', 'Un flujo para reconstruir decisiones y contexto repartidos entre tus herramientas.', ['El trabajo rara vez vive en un solo lugar. Una decisión puede empezar en Slack, aclararse por Gmail y terminar en Notion.', 'Pregunta por la decisión, la persona responsable y el siguiente paso, no solo por el nombre de la aplicación.', 'Las respuestas deben incluir fuentes y pedir aprobación antes de enviar o cambiar algo.']],
    'investors-on-the-memory-and-context-layer': ['Qué dicen los inversores sobre memoria y contexto', 'Una síntesis sobre contexto, memoria y agentes que trabajan como compañeros.', ['La ventaja de la IA no depende solo de modelos más grandes, sino de entender el contexto de cada trabajo.', 'La capa de contexto conecta entidades, relaciones, permisos y conocimiento tácito; la memoria conserva lo que seguirá siendo útil.', 'ShogunAI conecta memoria personal y ejecución aprobada.']],
    'passive-memory-not-screenshots': ['Memoria pasiva, no capturas', 'Cómo recuperar contexto privado sin convertir tu pantalla en un sistema de vigilancia.', ['Las capturas son un sustituto pobre del contexto: no explican qué importó ni qué debe ocurrir después.', 'La memoria pasiva debe ser selectiva, local-first, buscable y fácil de pausar, borrar y controlar.']],
    'shogunai-vs-glean': ['ShogunAI frente a Glean', 'La diferencia entre memoria personal de trabajo y búsqueda empresarial.', ['Glean se centra en descubrir conocimiento de una organización. ShogunAI se centra en reconstruir el día de una persona.', 'Elige según necesites una base de búsqueda para toda la empresa o continuidad para tu propio trabajo.']],
    'shogunai-vs-mem': ['ShogunAI frente a Mem', 'Memoria de contexto de trabajo frente a notas personales asistidas por IA.', ['Mem parte de las notas que escribes. ShogunAI parte del contexto que aparece en conversaciones, correo, documentos y decisiones.', 'Las notas curadas y la memoria de trabajo pueden convivir y cubrir necesidades distintas.']],
    'shogunai-vs-notion': ['ShogunAI frente a Notion', 'Memoria de contexto frente a espacio de trabajo.', ['Notion sirve para escribir, organizar y compartir. ShogunAI recupera el contexto que rodea esas páginas y lo lleva al siguiente paso.', 'No siempre se sustituyen: pueden dividir el conocimiento formal y el contexto cotidiano.']],
    'the-execution-layer': ['La capa de ejecución', 'Cómo la memoria se convierte en trabajo terminado con aprobación humana.', ['La memoria vale cuando reduce el trabajo posterior. ShogunAI prepara borradores, planes y acciones en las herramientas que ya usas.', 'Enviar, publicar, cambiar o borrar requiere una revisión visible antes de producir consecuencias.']],
    'welcome-to-shogunai': ['Te damos la bienvenida a ShogunAI', 'Por qué construimos un sistema operativo para la persona AI-native.', ['El contexto importante vive entre conversaciones, pestañas, decisiones y borradores. ShogunAI lo hace recuperable sin reconstruir la historia.', 'La privacidad local-first, la elección del proveedor y la aprobación de acciones son principios del producto.']],
    'why-local-first': ['Por qué local-first', 'Qué significa mantener la memoria de tu trabajo en tu propio dispositivo.', ['La memoria personal contiene decisiones, relaciones y documentos que no deberían convertirse en una copia permanente en la nube.', 'El almacenamiento local reduce copias innecesarias y deja en tus manos cuándo compartir el contexto con un proveedor de IA.']],
  },
  de: {
    'ai-memory-context-layer-guide': ['Warum KI Gedächtnis braucht', 'Ein praktischer Leitfaden zu Gedächtnis, Kontext, Datenschutz und Ausführung.', ['Ein kluges Modell beginnt trotzdem bei null, wenn es den Arbeitskontext nicht kennt. Gedächtnis bewahrt Entscheidungen und Präferenzen; Kontext stellt die Informationen für die aktuelle Aufgabe zusammen.', 'Eine gute Schicht braucht selektive Erfassung, Quellen, klare Berechtigungen und Pausieren- sowie Löschkontrollen.', 'ShogunAI verbindet local-first Gedächtnis mit Ausführung und Freigabe.']],
    'best-ai-memory-tools-for-knowledge-workers': ['KI-Gedächtnis für Wissensarbeit auswählen', 'Ein praktischer Leitfaden für Entscheidungen, Gespräche und Dokumente im Arbeitsalltag.', ['Gute Werkzeuge speichern nicht nur Notizen, sondern holen den Kontext zurück, der zwischen Apps verloren geht.', 'Achte auf Erfassungsgrenzen, Quersuche, überprüfbare Quellen und Freigaben vor Aktionen.', 'ShogunAI behandelt persönlichen Arbeitskontext als private local-first Schicht.']],
    'how-to-search-slack-gmail-notion': ['Slack, Gmail und Notion gemeinsam durchsuchen', 'Ein Workflow für verteilte Entscheidungen und Arbeitskontext.', ['Arbeit liegt selten an einem Ort. Eine Entscheidung kann in Slack beginnen, per Gmail geklärt und in Notion dokumentiert werden.', 'Frage nach Entscheidung, Verantwortlichen und nächstem Schritt statt nur nach dem App-Namen.', 'Ergebnisse brauchen Quellen; Änderungen und Nachrichten warten auf deine Freigabe.']],
    'investors-on-the-memory-and-context-layer': ['Was Investoren über Gedächtnis und Kontext sagen', 'Ein Überblick über Kontext, Gedächtnis und Agenten als Kollegen.', ['Der Wettbewerb der KI hängt nicht nur von größeren Modellen ab, sondern davon, ob sie Arbeitskontext verstehen.', 'Die Kontextschicht verbindet Entitäten, Beziehungen, Berechtigungen und implizites Wissen; Gedächtnis bewahrt langfristig nützliche Informationen.', 'ShogunAI verbindet persönliches Gedächtnis mit freigegebener Ausführung.']],
    'passive-memory-not-screenshots': ['Passives Gedächtnis statt Screenshots', 'Arbeitskontext privat zurückholen, ohne den Bildschirm in einen Überwachungsfeed zu verwandeln.', ['Screenshots sind ein grober Ersatz für Kontext und erklären weder Wichtiges noch den nächsten Schritt.', 'Passives Gedächtnis sollte selektiv, lokal-first, durchsuchbar und leicht pausierbar und löschbar sein.']],
    'shogunai-vs-glean': ['ShogunAI und Glean im Vergleich', 'Persönliches Arbeitsgedächtnis oder Unternehmenssuche?', ['Glean konzentriert sich auf Wissenssuche in Organisationen. ShogunAI rekonstruiert den Arbeitstag einer einzelnen Person.', 'Die Wahl hängt davon ab, ob du eine Unternehmensbasis oder persönliche Kontinuität brauchst.']],
    'shogunai-vs-mem': ['ShogunAI und Mem im Vergleich', 'Arbeitskontext-Gedächtnis oder persönliche KI-Notizen?', ['Mem beginnt bei Notizen, die du selbst schreibst. ShogunAI beginnt bei Kontext in Gesprächen, E-Mails, Dokumenten und Entscheidungen.', 'Kuratiertes Wissen und passives Arbeitsgedächtnis können sich ergänzen.']],
    'shogunai-vs-notion': ['ShogunAI und Notion im Vergleich', 'Gedächtnisschicht oder Arbeitsbereich?', ['Notion dient zum Schreiben, Organisieren und Teilen. ShogunAI holt den Kontext rund um diese Seiten zurück und bringt ihn zum nächsten Schritt.', 'Beide können formales Wissen und täglichen Arbeitskontext gemeinsam abdecken.']],
    'the-execution-layer': ['Die Ausführungsschicht', 'Wie Gedächtnis mit menschlicher Freigabe zu fertiger Arbeit wird.', ['Gedächtnis ist wertvoll, wenn es Folgearbeit reduziert. ShogunAI erstellt Entwürfe, Pläne und Aktionen in deinen bestehenden Tools.', 'Senden, Veröffentlichen, Ändern und Löschen bleiben vor der Ausführung überprüfbar.']],
    'welcome-to-shogunai': ['Willkommen bei ShogunAI', 'Warum wir ein Betriebssystem für den AI-native Einzelnen bauen.', ['Wichtiger Kontext liegt zwischen Gesprächen, Tabs, Entscheidungen und Entwürfen. ShogunAI macht ihn wieder auffindbar.', 'Local-first Datenschutz, Anbieterwahl und Freigabe wichtiger Aktionen sind Produktprinzipien.']],
    'why-local-first': ['Warum local-first', 'Was es bedeutet, dein Arbeitsgedächtnis auf deinem eigenen Gerät zu halten.', ['Persönliches Gedächtnis enthält Entscheidungen, Beziehungen und Dokumente, die nicht dauerhaft in der Cloud liegen sollten.', 'Lokale Speicherung reduziert Kopien und lässt dich selbst entscheiden, wann ein KI-Anbieter Kontext erhält.']],
  },
};

function parseName(nameNoExt) {
  // This legacy file is Japanese content without a locale suffix. Keep its
  // public slug while treating it as the Japanese variant.
  if (nameNoExt === 'ai-memory-context-layer-guide') {
    return { slug: nameNoExt, locale: 'ja' };
  }
  const dot = nameNoExt.lastIndexOf('.');
  if (dot > 0) {
    const maybeLocale = nameNoExt.slice(dot + 1);
    if (LOCALES.has(maybeLocale)) {
      return { slug: nameNoExt.slice(0, dot), locale: maybeLocale };
    }
  }
  return { slug: nameNoExt, locale: 'en' };
}

function readingMinutes(text) {
  const words = text.trim().split(/\s+/).length;
  return Math.max(1, Math.round(words / 200));
}

const entries = [];
const files = readdirSync(BLOG_DIR).filter((file) => file.endsWith('.mdx')).sort();

for (const file of files) {
  const raw = readFileSync(join(BLOG_DIR, file), 'utf8');
  const { data, content } = matter(raw);
  const { slug, locale } = parseName(file.replace(/\.mdx$/, ''));
  const compiled = await evaluate(content, {
    ...runtime,
    baseUrl: import.meta.url,
    format: 'md',
    development: false,
  });

  entries.push({
    slug,
    locale,
    title: String(data.title ?? slug),
    description: String(data.description ?? ''),
    date: String(data.date ?? '1970-01-01'),
    category: String(data.category ?? 'Product'),
    author: String(data.author ?? 'ShogunAI'),
    image: String(data.image ?? CATEGORY_IMAGES[String(data.category ?? 'Product')] ?? CATEGORY_IMAGES.Product),
    readingMinutes: readingMinutes(content),
    html: renderToStaticMarkup(createElement(compiled.default)),
  });
}

// Some articles are authored in MDX only once. Materialize their localized
// editorial copy here so every published slug has a real body for every
// supported locale, including static generation and related-card metadata.
const sourceEntries = [...entries];
const escapeHtml = (value) => String(value)
  .replaceAll('&', '&amp;')
  .replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;')
  .replaceAll('"', '&quot;')
  .replaceAll("'", '&#39;');

for (const base of sourceEntries.filter((entry) => entry.locale === 'en')) {
  for (const locale of ['ja', 'es', 'de']) {
    if (entries.some((entry) => entry.slug === base.slug && entry.locale === locale)) continue;
    const copy = LOCALIZED_ARTICLE_COPY[locale]?.[base.slug];
    if (!copy) continue;
    const paragraphs = copy[2];
    entries.push({
      ...base,
      locale,
      title: copy[0],
      description: copy[1],
      readingMinutes: Math.max(1, Math.ceil(paragraphs.join(' ').split(/\s+/).length / 200)),
      html: paragraphs.map((paragraph) => `<p>${escapeHtml(paragraph)}</p>`).join('\n'),
    });
  }
}

const source = `/* This file is generated by scripts/generate-blog-data.mjs. */\nexport const BLOG_DATA = ${JSON.stringify(entries, null, 2)} as const;\n`;
writeFileSync(OUTPUT_FILE, source);

console.log(`Generated ${entries.length} localized blog records.`);
