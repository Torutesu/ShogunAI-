import { Logo } from './Logo';

export function Nav() {
  return (
    <header className="nav">
      <div className="container nav__inner">
        <a className="brand" href="/#top" aria-label="ShogunAI home">
          <Logo size={26} className="brand__mark" />
          <span className="brand__name">ShogunAI</span>
        </a>
        <nav className="nav__links" aria-label="Primary">
          <a href="/#memory">Memory</a>
          <a href="/#action">Action</a>
          <a href="/#how">How it works</a>
          <a href="/#pricing">Pricing</a>
        </nav>
        <div className="nav__cta">
          <a href="/#get-started" className="btn btn-secondary nav__signin">Sign in</a>
          <a href="/#get-started" className="btn btn-primary">Get started</a>
        </div>
      </div>
    </header>
  );
}
