const STATUS_ICON: Record<string, string> = {
  completed: '✓',
  in_progress: '◉',
  pending: '○',
};

export function TodoView({ todos }: { todos: { step: string; status: string }[] }) {
  return (
    <div className="turn-segment todo-card">
      <div className="todo-header">Tasks</div>
      <ul className="todo-list">
        {todos.map((t, i) => (
          <li key={i} className={`todo-item todo-${t.status}`}>
            <span className="todo-icon">{STATUS_ICON[t.status] || '○'}</span>
            <span className="todo-step">{t.step}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
