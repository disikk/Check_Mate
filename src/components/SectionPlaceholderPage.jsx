export default function SectionPlaceholderPage({ eyebrow, title, description }) {
  return (
    <div className="page-shell">
      <section className="bento-card page-intro-card">
        <div>
          <div className="page-eyebrow">{eyebrow}</div>
          <h1 className="page-heading">{title}</h1>
          <p className="page-description">{description}</p>
        </div>
        <div className="placeholder-badge">В разработке</div>
      </section>
    </div>
  )
}
