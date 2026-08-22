import type { Locale } from '@/i18n/config';
import { localizedMarketingContent } from './marketing-content-locales';

export type MarketingDetail = {
  slug: string;
  eyebrow: string;
  title: string;
  description: string;
  intro: string;
  highlights: readonly { title: string; body: string }[];
  steps: readonly { title: string; body: string }[];
  outcomes: readonly string[];
  faq: readonly (readonly [string, string])[];
};

export const featurePages: readonly MarketingDetail[] = [
  {
    slug: 'ai-memory',
    eyebrow: 'AI memory',
    title: 'It learns your work by watching you work',
    description: 'ShogunAI reads the text macOS already exposes to screen readers, keeps it encrypted on your Mac with its source and time, and indexes it on-device.',
    intro: 'The useful part of a workday is never in one document. It is spread across a thread you skimmed, a decision you made out loud, a draft you abandoned, and a document someone else edited. ShogunAI keeps that as a memory you did not have to maintain — and keeps it where you can see exactly what it holds.',
    highlights: [
      { title: 'Text first, through the accessibility layer', body: 'The focused window\u2019s app, title and visible text, read the way a screen reader reads it. Excluded apps, private windows and password fields are never read, and pause stops capture entirely — those exclusions apply before anything is written, not as a filter over stored data.' },
      { title: 'Screenshots only where text fails', body: 'Visual recall is off until you turn it on. With it on, a compressed screenshot is taken only for a window that yields no text at all — a chart in a shared slide, a canvas dashboard, a scanned PDF. It stays encrypted on the Mac and deletes itself at the retention age you choose, one to seven days or a bounded custom duration.' },
      { title: 'Indexed on your Mac', body: 'A multilingual embedding model ships with the app, so indexing a dense day costs nothing extra and recall works with no network. There is no per-token meter on your own memory, which is why we never have to remember less of your day to make the product cheaper.' },
      { title: 'Every memory carries its source', body: 'Entries link back to the event that produced them. That is what lets recall answer "Tuesday 14:20, in the design doc" instead of a confident guess, and it is what keeps low-confidence guesses out of anything you send.' },
    ],
    steps: [
      { title: 'Grant accessibility once', body: 'macOS asks, you approve, and capture follows the window you are actually working in. Nothing is read from apps you excluded.' },
      { title: 'Work as usual', body: 'Context accumulates without note-taking. Tiered storage keeps the last day instantly available, the last month in full detail, and older history compressed.' },
      { title: 'Inspect and prune', body: 'Delete an entry and it leaves search, storage and any context assembled for a later action. Pause whenever you want; captured nothing means nothing stored.' },
    ],
    outcomes: ['Recover the reasoning behind a decision weeks later', 'Find the source of a number instead of re-deriving it', 'Return to interrupted work without rebuilding the story', 'Keep sensitive context on the device by default'],
    faq: [
      ['Does ShogunAI take screenshots?', 'Only where text capture returns nothing, and only if you switch visual recall on — it is off by default. Those screenshots stay encrypted on your Mac, can be viewed or deleted at any time, and are deleted automatically at the retention age you pick. Everywhere else, capture is text through the accessibility layer and no image is written.'],
      ['Is meeting audio stored?', 'No. Audio streams to a speech service for live transcription while a meeting runs, is never used to train anyone\u2019s models, and is never written to disk or a temp file. What persists is the transcript text and where it came from, and the traceability log records the egress.'],
      ['Where does memory live?', 'In an encrypted database on your Mac. There is no server-side copy of your day to seize, breach, or quietly train on. What leaves the device is the specific slice a request needs, logged with its purpose.'],
      ['Does a heavy day cost more?', 'No. Embeddings run locally, so indexing is free whatever the volume, and recall works offline.'],
    ],
  },
  {
    slug: 'contextual-recall',
    eyebrow: 'Contextual recall',
    title: 'Ask about your week without pasting the backstory',
    description: 'One question across mail, chat, docs and calendar, answered from your own history with the source and time attached to every hit.',
    intro: 'A general assistant starts every session empty, so you spend the first few minutes re-explaining your life before you can ask anything useful. ShogunAI starts from the memory it already built, which means the question can be the short one you actually had.',
    highlights: [
      { title: 'Ask the way you remember', body: 'By person, project, decision, or roughly when — not only by exact keyword. Hybrid search runs full-text and vector retrieval together, so a half-remembered phrase and a fuzzy description both land.' },
      { title: 'One question, every connected tool', body: 'Mail, chat, documents and calendar answered from a single search instead of four. What you authorized is in scope; what you did not stays out.' },
      { title: 'Answers you can check', body: 'Every hit carries its source and its time. You get "Tuesday 14:20, in the design doc" and can open it, rather than trusting a paragraph with nothing behind it.' },
      { title: 'Confidence is visible', body: 'Each piece of state the system holds — people, projects, commitments, open loops — carries a confidence score. A low-confidence read surfaces as a question, never as a fact inside something you are about to send.' },
    ],
    steps: [
      { title: 'Ask', body: 'Describe what you need in ordinary language, even if you only remember part of it.' },
      { title: 'Retrieve', body: 'Recall searches the warm tier — the last month in full detail — and returns hits with provenance attached.' },
      { title: 'Continue', body: 'Turn the answer into a brief, a reply, or a next action without starting the context over.' },
    ],
    outcomes: ['Walk into a meeting already knowing where it left off', 'Recover a decision after two weeks away', 'Stop searching four apps for one thread', 'Draft from facts instead of from memory'],
    faq: [
      ['How is this different from search?', 'Search expects the exact words and one app at a time. Recall retrieves by meaning across the tools you connected, and attaches the source and time to each result so the answer is checkable.'],
      ['Does it work offline?', 'Yes for recall. Indexing and search run on your Mac, so finding things needs no network. Generation — drafting a reply, reasoning through a hard question — is the part that needs a model.'],
      ['What if it is not sure?', 'Low-confidence state is hedged rather than asserted. It comes back as "you may not have sent Mika the revised numbers", not as a sentence quietly hardened inside a draft.'],
      ['Which plan includes it?', 'Standard. Capture, recall, the daily brief and the first-layer read-only connections are all in the lower plan.'],
    ],
  },
  {
    slug: 'execution-layer',
    eyebrow: 'Execution layer',
    title: 'Answering is the easy half',
    description: 'Drafts, updates and actions inside the tools you already use, under three named levels of autonomy — and a stop before anything reaches another person.',
    intro: 'Most memory products answer well and leave you to do the work: copy the draft, open the mail client, fix the recipient, hunt for the file, attach it, send it. The execution layer is the part that closes that gap, and the rules around it are visible before anything runs.',
    highlights: [
      { title: 'Press Option, and it writes at your caret', body: 'Inline composition reads the field around your cursor and the memory behind it, then writes the continuation straight into the app you are already typing in. It is a device-local write: nothing is sent, and you send it yourself.' },
      { title: 'Three levels, and the line does not move', body: 'Level one is reversible and local, and just runs. Level two is drafted and waits for you. Level three is anything that reaches another person — sending mail, posting a message, putting an event on someone\u2019s calendar — and it always stops for approval. No prompt moves an action between levels.' },
      { title: 'Your plan or your key', body: 'Execution runs on the assistant subscription you already pay for, inside that plan\u2019s limits, or on an API key you bring. You pick the provider and can switch without losing a day of memory. Keys live in the system keychain and nowhere else.' },
      { title: 'A record of what it did', body: 'Every action leaves what ran, on what evidence, and what left the device — marked when it passed through a third party. That log is what makes level one acceptable: automation you can audit afterwards is a different proposition from automation you have to trust in advance.' },
    ],
    steps: [
      { title: 'Understand', body: 'The request is read against the state of your work — who is involved, what was decided, what is still open — rather than the thread alone.' },
      { title: 'Prepare', body: 'The draft, update or tool action is assembled with the right file already attached and the open question already answered.' },
      { title: 'Approve', body: 'Anything consequential waits on one approval. Reversible work has already finished by the time you look.' },
    ],
    outcomes: ['Send the follow-up with the right version attached', 'Walk into the 3pm already prepped', 'Close the loop you forgot was open', 'Keep the send button in your hands'],
    faq: [
      ['Can it send something without asking?', 'No. Anything that reaches another person is level three and stops for approval. That is a property of how actions are routed, not a setting you have to get right.'],
      ['Does driving it over the API skip the gate?', 'No. The same classifier and the same gates apply over MCP, CLI and REST. An agent calling in has no more authority than your own click.'],
      ['Do I need an API key?', 'No. It can run on the assistant plan you already pay for. Bringing your own key is the alternative, not the requirement.'],
      ['Which plan includes it?', 'Pro, along with the Memory API and the second-layer connections. Standard covers capture, recall and everyday execution.'],
    ],
  },
] as const;

export const useCasePages: readonly MarketingDetail[] = [
  {
    slug: 'founders',
    eyebrow: 'For founders',
    title: 'Keep the company context you cannot afford to lose',
    description: 'ShogunAI helps founders recall decisions, prepare investor and team updates, and turn scattered company context into action.',
    intro: 'A founder moves between product, hiring, customers, fundraising, and operations every day. ShogunAI keeps the context behind those switches available, so the next conversation starts with what already happened.',
    highlights: [
      { title: 'Investor preparation', body: 'Bring prior updates, metrics discussions, open questions, and commitments into one preparation flow.' },
      { title: 'Decision history', body: 'Recover why the team chose a direction—not only the final task or document.' },
      { title: 'Follow-through', body: 'Turn meetings and conversations into drafts, updates, and clearly owned next actions.' },
    ],
    steps: [
      { title: 'Capture the operating context', body: 'Build memory across the projects, conversations, and research that shape the company.' },
      { title: 'Ask before the next decision', body: 'Recall previous commitments, objections, and assumptions in natural language.' },
      { title: 'Ship the follow-up', body: 'Prepare the update or response from the same context and approve the final action.' },
    ],
    outcomes: ['Prepare board and investor updates', 'Resume fundraising conversations accurately', 'Keep hiring context across interviews', 'Reduce founder context switching'],
    faq: [
      ['Is ShogunAI a company knowledge base?', 'ShogunAI is primarily a private memory and execution layer for the individual. It complements shared company knowledge by preserving the context behind your own work.'],
      ['Can it help with investor meetings?', 'Yes. It can help retrieve prior discussions, questions, commitments, and related work to prepare a grounded briefing.'],
      ['Does my team need to change tools?', 'No. ShogunAI is designed to work across existing tools rather than require a complete workflow migration.'],
    ],
  },
  {
    slug: 'product-engineering',
    eyebrow: 'For product & engineering',
    title: 'Carry product context from discussion to delivery',
    description: 'Recall technical decisions, customer evidence, design trade-offs, and project history without searching every tool separately.',
    intro: 'Product work loses time at the seams: a customer request in email, a decision in Slack, a design in Figma, and an issue in Linear. ShogunAI creates a personal context layer across those moments.',
    highlights: [
      { title: 'Decision recall', body: 'Find the reasoning, alternatives, and constraints behind a product or technical choice.' },
      { title: 'Faster handoffs', body: 'Prepare concise context for teammates without manually reconstructing every source.' },
      { title: 'Project continuity', body: 'Return to interrupted work with the relevant history and next steps already available.' },
    ],
    steps: [
      { title: 'Follow the work', body: 'Capture relevant context as work moves between research, discussion, design, and implementation.' },
      { title: 'Retrieve the why', body: 'Ask about a feature, bug, customer, or decision using the language your team already uses.' },
      { title: 'Create the artifact', body: 'Turn that context into a brief, issue, update, or handoff for review.' },
    ],
    outcomes: ['Write better product briefs', 'Recover technical rationale', 'Prepare sprint and launch updates', 'Onboard yourself back into old projects'],
    faq: [
      ['Does this replace Linear, Jira, or Notion?', 'No. ShogunAI is a context and execution layer across your existing tools, not a replacement project-management system.'],
      ['Can it connect code and product context?', 'It is designed to relate work across the tools you authorize, helping you recall the discussions and artifacts surrounding implementation.'],
      ['Is it useful for individual contributors?', 'Yes. The product is designed around an individual’s private work memory, including engineers, designers, and product managers.'],
    ],
  },
  {
    slug: 'consultants',
    eyebrow: 'For consultants',
    title: 'Remember every client without carrying every detail in your head',
    description: 'Keep client-specific context separate, prepare faster, and create follow-ups grounded in the work already completed.',
    intro: 'Client work demands rapid switching between companies, people, terminology, and commitments. ShogunAI helps you recover the right context before a call and turn it into deliverables afterward.',
    highlights: [
      { title: 'Sales professionals', body: 'Turn pricing feedback from today’s call and technical requirements from last month into a client-ready proposal without rebuilding the context.' },
      { title: 'Consultants', body: 'Create a project plan from the scope, kickoff feedback, and discovery notes already in your work history.' },
      { title: 'Account managers', body: 'Recall the last client touchpoint, open request, and owner across your team without digging through every system.' },
    ],
    steps: [
      { title: 'Build private client memory', body: 'Capture the context you need while keeping your personal work layer local-first.' },
      { title: 'Prepare before the call', body: 'Recall recent changes, commitments, and open decisions in one query.' },
      { title: 'Deliver after the call', body: 'Create a clear recap, plan, or client-ready draft and approve it before sending.' },
    ],
    outcomes: ['End the hunt for client context scattered across inboxes, documents, meetings, and notes', 'Prepare proposals, follow-ups, and reports from context you already have', 'Stay present in client calls while ShogunAI keeps the thread', 'Switch between clients with less mental overhead', 'Keep private client context local by default and control what is shared'],
    faq: [
      ['Can I keep different client contexts separate?', 'ShogunAI is designed around controlled, searchable work context. You decide what is captured and which connected services are authorized.'],
      ['Will it send client emails automatically?', 'Consequential actions use approval gates, so you can review a client-facing message before it is sent.'],
      ['Is the memory cloud-only?', 'No. ShogunAI is local-first by default, with optional provider sharing only where an enabled feature requires it.'],
    ],
  },
] as const;

export function findMarketingPage(pages: readonly MarketingDetail[], slug: string) {
  return pages.find((page) => page.slug === slug);
}

export function getFeaturePages(locale: Locale = 'en'): readonly MarketingDetail[] {
  return locale === 'en' ? featurePages : localizedMarketingContent[locale].features;
}

export function getUseCasePages(locale: Locale = 'en'): readonly MarketingDetail[] {
  return locale === 'en' ? useCasePages : localizedMarketingContent[locale].useCases;
}
