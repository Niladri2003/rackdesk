// The landing page script: the copy buttons, the diagram of the server and
// the helper, the start sequence of the hero, and the light that follows the
// pointer over the app cards. The page is complete without it.

const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches

// ---------- Copy buttons ----------

for (const button of document.querySelectorAll('.copy')) {
  button.addEventListener('click', async () => {
    const source = document.getElementById(button.dataset.copy)
    if (!source) return
    try {
      await navigator.clipboard.writeText(source.textContent.trim())
      button.textContent = 'Copied'
      button.classList.add('done')
    } catch {
      // The browser refused access to the clipboard. Select the text, so the
      // visitor can copy it with the keyboard.
      const range = document.createRange()
      range.selectNodeContents(source)
      const selection = window.getSelection()
      selection.removeAllRanges()
      selection.addRange(range)
      button.textContent = 'Selected'
    }
    setTimeout(() => {
      button.textContent = 'Copy'
      button.classList.remove('done')
    }, 2000)
  })
}

/// Sets "in" on a part while it is on screen, and calls `change`.
const watch = (selector, change) => {
  const part = document.querySelector(selector)
  if (!part) return
  new IntersectionObserver(
    ([entry]) => {
      part.classList.toggle('in', entry.isIntersecting)
      change(entry.isIntersecting)
    },
    { threshold: 0.25 },
  ).observe(part)
}

// ---------- The diagram of the server and the helper ----------
// It shows 1 request after the other while it is on screen. Some pass and
// some are refused.

const requests = [
  { ask: 'Change the owner of /srv/app', answer: 'Done. Written to the audit log.', ok: true },
  { ask: 'Run a command as root', answer: 'Refused. The helper has no such action.', ok: false },
  { ask: 'Read /var/log/auth.log', answer: 'Done. Written to the audit log.', ok: true },
  { ask: 'Change a file outside the allowed folder', answer: 'Refused. The path leaves the folder of the helper.', ok: false },
  { ask: 'Open a terminal', answer: 'Started as the user you chose. Written to the audit log.', ok: true },
]

const flow = document.getElementById('flow')
if (flow && !still) {
  const ask = document.getElementById('flow-req')
  const answer = document.getElementById('flow-res')
  let next = 0
  let timer = 0

  const show = () => {
    const request = requests[next]
    next = (next + 1) % requests.length
    ask.textContent = request.ask
    answer.textContent = request.answer
    flow.classList.remove('run')
    flow.classList.toggle('no', !request.ok)
    // Reading the size makes the browser drop the old animation, so that
    // adding "run" starts it again.
    void flow.offsetWidth
    flow.classList.add('run')
  }

  watch('#flow', (visible) => {
    clearInterval(timer)
    if (!visible) return
    show()
    timer = setInterval(show, 4600)
  })
}

// ---------- The hero picture ----------

const wide = window.matchMedia('(min-width: 701px)')
const hero = document.querySelector('.hero')
const stage = document.getElementById('stage')

if (stage && !still) {
  let queued = false
  const update = () => {
    queued = false
    // 1 at the top of the page, 0 after 520 px of scrolling.
    const tilt = Math.max(0, 1 - window.scrollY / 520)
    stage.style.setProperty('--tilt', tilt.toFixed(3))
  }
  window.addEventListener(
    'scroll',
    () => {
      if (queued) return
      queued = true
      requestAnimationFrame(update)
    },
    { passive: true },
  )
  update()
}

// ---------- The start sequence ----------
// A drawn pointer clicks Copy. The command is pasted in the terminal and its
// output prints. The pointer clicks the printed address and the browser
// opens. There the pointer signs in, opens Files and 2 folders, opens the
// Terminal, where a command is typed, and opens search.
//
// The browser shows screenshots of RackDesk. The pointer is a picture: a
// click changes the screenshot. It does not use the clipboard of the visitor.
// The sequence runs once, and again from the button under the picture. It
// does not run with reduced motion or on a small screen: there the last
// screenshot shows from the start.

const pointer = document.getElementById('pointer')
const button = document.getElementById('again')
const view = document.getElementById('view')

// Where the pointer clicks in a screenshot, as parts of its width and height.
const SPOT = {
  signIn: [0.5, 0.665],
  search: [0.022, 0.04],
  files: [0.022, 0.109],
  terminal: [0.022, 0.165],
  firstRow: [0.351, 0.194],
}
const COMMAND = 'ls etc/nginx'

if (hero && stage && pointer && view && !still && wide.matches) {
  const copy = hero.querySelector('.copy')
  const shell = stage.querySelector('.shell')
  const pasted = stage.querySelector('.pasted')
  const lines = [...stage.querySelectorAll('.o')]
  const address = document.getElementById('shell-url')
  const typed = document.getElementById('typed')
  const frames = new Map()

  /// Adds the screenshots to the page. The build wrote their addresses into
  /// `data-frames`. They load while the terminal part of the sequence plays.
  const loadFrames = () => {
    if (frames.size) return
    for (const [name, picture] of Object.entries(JSON.parse(view.dataset.frames))) {
      const img = new Image()
      img.className = 'shot frame'
      img.alt = ''
      img.sizes = view.dataset.sizes
      img.srcset = picture.srcset
      img.src = picture.src
      view.insertBefore(img, typed)
      frames.set(name, img)
    }
  }

  // Each run has a number. A run stops when a newer one has started.
  let run = 0
  let playing = false
  const wait = (ms, mine) =>
    new Promise((done, stop) => setTimeout(() => (mine === run ? done() : stop(new Error('replaced'))), ms))

  /// Shows a screenshot, once the browser has it ready.
  const show = async (name, mine) => {
    const img = frames.get(name)
    await img.decode().catch(() => {})
    if (mine !== run) throw new Error('replaced')
    img.classList.add('on')
  }

  /// Moves the pointer to a place inside an element. `x` and `y` are parts of
  /// its width and height, from 0 to 1.
  const moveTo = (target, x = 0.5, y = 0.5) => {
    const from = hero.getBoundingClientRect()
    const box = target.getBoundingClientRect()
    const left = box.left - from.left + box.width * x
    const top = box.top - from.top + box.height * y
    pointer.style.transform = `translate(${left.toFixed(1)}px, ${top.toFixed(1)}px)`
  }

  const press = async (mine) => {
    pointer.classList.add('press')
    await wait(160, mine)
    pointer.classList.remove('press')
  }

  /// Moves to a place in the screenshot and clicks there, 1 or 2 times.
  const click = async (spot, mine, times = 1) => {
    moveTo(view, spot[0], spot[1])
    await wait(900, mine)
    for (let i = 0; i < times; i++) {
      await press(mine)
      await wait(110, mine)
    }
  }

  const clear = () => {
    stage.classList.remove('s-view')
    shell.classList.remove('focus')
    pasted.classList.remove('show')
    address.classList.remove('hit')
    typed.classList.remove('on')
    typed.textContent = ''
    for (const line of lines) line.classList.remove('show')
    for (const img of frames.values()) img.classList.remove('on')
    pointer.classList.remove('on', 'press')
  }

  /// Ends the sequence and shows the finished picture.
  const finish = () => {
    run += 1
    playing = false
    clear()
    stage.classList.remove('seq')
    button.textContent = 'Play again'
  }

  const play = async () => {
    const mine = ++run
    playing = true
    clear()
    stage.classList.add('seq')
    loadFrames()
    frames.get('signin').classList.add('on')
    button.textContent = 'Skip'
    button.hidden = false
    moveTo(hero.querySelector('.lead'), 0.85, 1.2)
    await wait(700, mine)
    pointer.classList.add('on')
    await wait(350, mine)

    // Copy the command.
    moveTo(copy)
    await wait(850, mine)
    await press(mine)
    copy.textContent = 'Copied'
    copy.classList.add('done')
    setTimeout(() => {
      copy.textContent = 'Copy'
      copy.classList.remove('done')
    }, 2200)
    await wait(550, mine)

    // Paste it in the terminal. A paste puts the whole command in at once.
    moveTo(shell, 0.42, 0.3)
    await wait(850, mine)
    await press(mine)
    shell.classList.add('focus')
    await wait(450, mine)
    pasted.classList.add('show')
    await wait(800, mine)
    for (const line of lines) {
      await wait(line.classList.contains('gap') ? 620 : 300, mine)
      line.classList.add('show')
    }
    await wait(700, mine)

    // Open the printed address. The browser opens with the sign-in page. A
    // visitor who is still at the top of the page is moved down to it, so
    // the whole browser is in view.
    moveTo(address, 0.5, 0.6)
    await wait(850, mine)
    address.classList.add('hit')
    await press(mine)
    await wait(300, mine)
    stage.classList.add('s-view')
    if (window.scrollY < 80) {
      const top = stage.getBoundingClientRect().top + window.scrollY - 110
      window.scrollTo({ top, behavior: 'smooth' })
    }
    await wait(1200, mine)

    // Sign in.
    await click(SPOT.signIn, mine)
    await show('desk', mine)
    await wait(500, mine)

    // Files: open it, then the folders etc and nginx.
    await click(SPOT.files, mine)
    await show('files-0', mine)
    await wait(350, mine)
    await click(SPOT.firstRow, mine, 2)
    await show('files-1', mine)
    await wait(500, mine)
    await press(mine)
    await wait(110, mine)
    await press(mine)
    await show('files-2', mine)
    await wait(700, mine)

    // The Terminal: open it and type a command.
    await click(SPOT.terminal, mine)
    await show('term-0', mine)
    await wait(600, mine)
    typed.classList.add('on')
    for (const char of COMMAND) {
      typed.textContent += char
      await wait(char === ' ' ? 130 : 70, mine)
    }
    await wait(450, mine)
    await show('term-1', mine)
    typed.classList.remove('on')
    await wait(900, mine)

    // Search.
    await click(SPOT.search, mine)
    await show('search-0', mine)
    await wait(900, mine)
    await show('search-1', mine)
    pointer.classList.remove('on')
    await wait(2600, mine)
    frames.get('search-1').classList.remove('on')
    frames.get('search-0').classList.remove('on')
    await wait(500, mine)
    finish()
  }

  button.addEventListener('click', () => {
    if (playing) finish()
    else play().catch(() => {})
  })
  // A visitor who uses the real Copy button during the sequence keeps the
  // normal behaviour of that button: main.js handles the click.
  play().catch(() => {})
}

for (const card of document.querySelectorAll('.card')) {
  card.addEventListener('pointermove', (event) => {
    const box = card.getBoundingClientRect()
    card.style.setProperty('--mx', `${event.clientX - box.left}px`)
    card.style.setProperty('--my', `${event.clientY - box.top}px`)
  })
}
