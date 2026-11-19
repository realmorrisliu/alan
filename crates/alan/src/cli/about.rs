//! Render the `alan about` ANSI card (Klein Blue Braille mascot + ADR-0061 namespace overview).

use alan_tui::theme::{Theme, ThemeMode};

const ASCII_RABBIT: &str = "\
  (\\_/)  +-------+\n\
 =(o.o)= | /proc |\n\
  / >[]  |_______|\n\
 (__)__) ==o===o==";

/// Braille mascot from `website/worker/index.js`, parameterized for color mode downgrade.
const BRAILLE_MASCOT_TEMPLATE: &str = "\
                  {W}⡠⡠⡤⡤⡤⡀{X}
                 {W}⡠⡾⡵⠃⠈⢿⢷⡄{X}     {W}⡾⠉{K}⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉{W}⠉⠉⢿⢷⣆⡀{X}
                 {W}⢸{S}⡕{W}⣿⡤⠤⠼⣿⢷{X}     {W}⡇{K}⢸⣿⣝⢿⣽⣽⣽⣽⣽⣽⣽⢿⣟{W}⣿⢈⡗⡇{X}  {W}⠉⠒⠤⡀{X}
                {W}⡠⣚⣵{K}⣿⠁{X}   {W}⠈⠳⣄{X}   {W}⡇{K}⣿⢿⣽⠒⣖⣖⣦⣟⣟⣟⣟⣯{S}⡅{W}⡇⢺⡇{X}     {W}⠻{X}
               {W}⢸⡇{X} {W}⡷⣿⢿{K}⢷{W}⡀⣀⡀{X} {W}⠈⣦{X} {W}⢸⠁{K}⣿{W}⡗{K}⢿{S}⡕⡕⡕⡕⡕⡕⡕⡕{K}⣽{S}⠑{W}⡇⢸⣽⡇{X}     {W}⢸{X}
              {W}⡠⠏{X} {W}⢸⣵⣯{K}⣿⣿{W}⡇⠻⠻{X}  {W}⠸⡄⢸⡠{K}⣿{S}⡕{K}⡗{S}⡕⡕⡕⡕⡕⡕⡕⠑{K}⢿{W}⣄⡇⣾⣽⠁{X}     {W}⢸{X}
             {W}⡀⠏{X}   {W}⣯⣿{K}⣿⠟{X}     {W}⠳⡇⢸⢸{K}⣿⡵⡗{S}⡕⡕⡕⡕⡕⠑⡕⣄{K}⡗{W}⣽⠁⢿⣽{X}      {W}⢸{X}
             {W}⢸⠁{X}   {W}⠸⠈⡧⡤⣀⣀{S}⡀{W}⡤⠖⠉{X} {W}⡜{K}⢸⣿⣽⠷⡵⡵⡵⡵⡵⡤⡴⡵⡗{W}⢸{X} {W}⡗⣿{X}      {W}⢸{X}
            {W}⡀⣾{X}   {W}⡀⣿{X}  {W}⠑⡦⡀{S}⠁{W}⡗{X}   {W}⡇{R}⢸⡟{W}⠽⠷⠷⠷⠷⡧⡼⡵⡼⡼⡵⠻⡠⡗⡇{X}      {W}⢸{X}
           {W}⡠⠋⢸⡄{X} {W}⡠⠟{S}⠁{X}    {W}⠈⠳⣽{X} {W}⡀⣀⡷{R}⡬⣵{K}⡤⡤⡤⣀⣀⣀{W}⣀⣀⣀⡐⠶⠴⢸⡗⡗⡤{K}⡤⡴⡦⣯{W}⠉⡜⡷⢟⡼⡦{X}
          {W}⡠⠋{X}  {W}⠙⠻⠁{S}⠁{X}      {W}⠈⠻⠽⠿⠿⣿⣿⣿⣿⣿⣿⡿{K}⣼⣿⢿{W}⣿{K}⣿⣿⣿{W}⣿⣿⡿⠴⢿{K}⣽{W}⢷⣽{X}
   {W}⡀{X}     {W}⡠⡇{X}          {W}⠑⣄⡀{X}   {W}⡀⡤⣾⣿⣿⠿⢿⣿⠳⣿⣿⡿⢟⡷⡷⣶⣿⣟⣯{K}⣿{W}⢿{K}⠿⡛{W}⡇⢸⡗⡼⡇⣾{K}⣀{X}
   {W}⠉⠉⠉⠉⠉⠉{X}          {W}⡠⠤⠤⠽⣿{K}⣯⣿⡤{W}⣴{K}⠉⠉{W}⠉⠉⠳⠒⠒⠒⣶⣯⣽{K}⡵⡵⣽{W}⢿{K}⠟⠻{W}⡤⠳⣄⡠⠟{K}⣿⢿{W}⠷⠻{K}⠋⠉{X}
     {W}⠉⠉⠉⠉⣯{X}               {W}⠹⠷⠒⠒⠊⢿{K}⠭⠭⠭⣯⢿⣯{W}⣯⣯{K}⠟⠋⠉{X}      {W}⠉⠉⠉⠉⠁{X}
         {W}⠸⣀{X}               {W}⠻⠙⠳⠖⠒⠦⠤⠤⠤⡤⣀⣀⠉⠉⠉⠉⠑⠙⠂{X}
        {W}⢸{X}  {W}⠁{X}              {W}⢸⠒⠙⣼{X}          {W}⠈⠉⠉⠁{X}
        {W}⠘⣆⣀⣀⣆⣄{X}            {W}⠋⡠⠟⠁{X}
        {W}⢸⣒{K}⢿{W}⡿{K}⣯⣯⣯⣿⡶⡶⡶⡶⡶{W}⣼⢿⢿⢷⠤⠟⠁{X}
        {W}⠘⠒⠤⡤⣀⣀⣀⣀⣀⣀⣀⣀⣀⡤⠤⠔⠋{X}
               {K}⢸⢿{W}⣿{X}
               {K}⢸{X} {K}⣿{X}
               {K}⠘{X} {K}⠻{X}";

/// Render the `alan about` card into a `String` based on environment color settings.
pub fn format_about_card() -> String {
    let dumb_env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
        || std::env::var("TERM")
            .ok()
            .is_some_and(|t| t.eq_ignore_ascii_case("dumb"))
        || std::env::var("ALAN_THEME")
            .ok()
            .is_some_and(|t| t.eq_ignore_ascii_case("dumb"));
    let theme = if dumb_env {
        Theme::dumb()
    } else {
        let parsed = Theme::from_env();
        if parsed.is_dumb {
            Theme::dumb()
        } else {
            parsed
        }
    };
    render_about_for_theme(theme)
}

/// Render `alan about` for an explicit [`Theme`], used by both runtime and tests.
pub fn render_about_for_theme(theme: Theme) -> String {
    if theme.is_dumb {
        return render_plain_about();
    }

    let (w, k, s, r, b, d, x) = match theme.mode {
        ThemeMode::Paper => (
            "\x1b[38;2;45;52;68m",
            "\x1b[38;2;0;47;167m",
            "\x1b[38;2;75;110;205m",
            "\x1b[38;2;217;56;46m",
            "\x1b[1m",
            "\x1b[2m",
            "\x1b[0m",
        ),
        ThemeMode::Ansi => (
            "\x1b[37m",
            "\x1b[34;1m",
            "\x1b[36m",
            "\x1b[31m",
            "\x1b[1m",
            "\x1b[2m",
            "\x1b[0m",
        ),
        ThemeMode::Auto | ThemeMode::Midnight | ThemeMode::Dumb => (
            "\x1b[38;2;214;212;204m",
            "\x1b[38;2;76;126;255m",
            "\x1b[38;2;120;156;235m",
            "\x1b[38;2;242;82;72m",
            "\x1b[1m",
            "\x1b[2m",
            "\x1b[0m",
        ),
    };

    let mascot = BRAILLE_MASCOT_TEMPLATE
        .replace("{W}", w)
        .replace("{K}", k)
        .replace("{S}", s)
        .replace("{R}", r)
        .replace("{X}", x);

    format!(
        "\n{mascot}\n\n\
  {b}{k}ALAN(1){x}  —  Programmable personal computing environment on {k}alan9{x}\n\
  Everything is a file. Even an agent.\n\n\
  {b}STANDARD NAMESPACE{x}\n\
    {k}/proc{x}    Process lifecycle truth\n\
    {k}/agent{x}   AgentFS views of Agent Processes (/agent/<pid>)\n\
    {k}/srv{x}     Mountable file-server handles (aP/1 protocol)\n\
    {k}/bin{x}     Tools and Agent Executables (/bin/alan, /bin/q)\n\
    {k}/lib{x}     Immutable projected Skill and package trees\n\
    {k}/man{x}     Read-only UNIX manual pages\n\
    {k}/mnt{x}     Explicitly authorized Host Mounts\n\n\
  {b}QUICKSTART{x}\n\
    alan connection add <provider> --profile <name>\n\
    alan\n\
    alan man alan9\n\n\
  {d}https://alanworks.app · https://github.com/realmorrisliu/alan · Apache-2.0{x}\n"
    )
}

fn render_plain_about() -> String {
    format!(
        "{ASCII_RABBIT}\n\n\
ALAN(1)  —  Programmable personal computing environment on alan9\n\
Everything is a file. Even an agent.\n\n\
STANDARD NAMESPACE\n\
  /proc    Process lifecycle truth\n\
  /agent   AgentFS views of Agent Processes (/agent/<pid>)\n\
  /srv     Mountable file-server handles (aP/1 protocol)\n\
  /bin     Tools and Agent Executables (/bin/alan, /bin/q)\n\
  /lib     Immutable projected Skill and package trees\n\
  /man     Read-only UNIX manual pages\n\
  /mnt     Explicitly authorized Host Mounts\n\n\
QUICKSTART\n\
  alan connection add <provider> --profile <name>\n\
  alan\n\
  alan man alan9\n\n\
https://alanworks.app · https://github.com/realmorrisliu/alan · Apache-2.0\n"
    )
}

/// Execute `alan about`.
pub fn run() -> anyhow::Result<()> {
    print!("{}", format_about_card());
    Ok(())
}
