% Report.cls – document class for timesheet.tex
\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{Report}

\RequirePackage{etoolbox}

\newtoggle{unnumberedsections}
\DeclareOption{unnumberedsections}{\toggletrue{unnumberedsections}}
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{article}}
\ProcessOptions\relax

\LoadClass{article}

\RequirePackage{graphicx}
\graphicspath{{Images/}{./}}

\RequirePackage{babel}
\exhyphenpenalty=10000
\doublehyphendemerits=100000

\providecommand{\pageoflabel}{of}
\AtBeginDocument{\iflanguage{ngerman}{\renewcommand{\pageoflabel}{von}}{}}

\setlength{\parindent}{0pt}
\setlength{\parskip}{6pt}

\RequirePackage[bottom, hang]{footmisc}
\setlength{\footnotemargin}{6pt}

\RequirePackage{lastpage}
\RequirePackage{appendix}
\RequirePackage{float}
\RequirePackage{multicol}
\RequirePackage{tabularx}

\AtBeginEnvironment{quote}{\itshape\rmfamily}

\RequirePackage[
	top        = 3cm,
	bottom     = 3cm,
	left       = 1.75cm,
	right      = 1.75cm,
	footskip   = 1.5cm,
	headsep    = 0.8cm,
	headheight = 1.5cm,
	columnsep  = 0.9cm,
]{geometry}

\RequirePackage{fontspec}
\defaultfontfeatures{Ligatures=TeX}

\IfFileExists{Fonts/Poppins/Poppins-Light.ttf}{
	\defaultfontfeatures[Poppins]{
		Path           = Fonts/Poppins/,
		UprightFont    = *-Light.ttf,
		BoldFont       = *-SemiBold.ttf,
		ItalicFont     = *-LightItalic.ttf,
		BoldItalicFont = *-SemiBoldItalic.ttf,
	}
	\setsansfont{Poppins}
	\setmainfont{Poppins}
	\newfontface{\tocfont}{Poppins-Regular.ttf}[Path=Fonts/Poppins/]
	\newfontface{\headingfont}{Poppins-Medium.ttf}[Path=Fonts/Poppins/]
}{
	\newcommand{\tocfont}{\normalfont}
	\newcommand{\headingfont}{\bfseries}
}

\renewcommand*\familydefault{\sfdefault}

\IfFileExists{Fonts/JetBrainsMono/JetBrainsMono-Light.ttf}{
	\setmonofont{JetBrainsMono}[
		Path           = Fonts/JetBrainsMono/,
		UprightFont    = *-Light.ttf,
		BoldFont       = *-SemiBold.ttf,
		ItalicFont     = *-LightItalic.ttf,
		BoldItalicFont = *-SemiBoldItalic.ttf,
	]
}{}

\RequirePackage{fancyhdr}
\pagestyle{fancy}
\fancyhf{}
\renewcommand{\headrulewidth}{0.5pt}
\fancyhead[L]{\leftheadercontent}
\fancyhead[R]{\rightheadercontent}
\fancyfoot[R]{\thepage~\pageoflabel{} \pageref{LastPage}}

\RequirePackage{titlesec}
\iftoggle{unnumberedsections}{\setcounter{secnumdepth}{0}}{\setcounter{secnumdepth}{3}}

\titleformat{\section}[block]{\raggedright\large\headingfont}{\thesection}{7pt}{}[]
\titlespacing*{\section}{0pt}{0.75\baselineskip}{4pt}

\titleformat{\subsection}[block]{\raggedright\headingfont}{\thesubsection}{5pt}{}[]
\titlespacing*{\subsection}{0pt}{0.75\baselineskip}{0pt}

\titleformat{\subsubsection}[block]{\raggedright\bfseries\small}{\thesubsubsection}{5pt}{}[]
\titlespacing*{\subsubsection}{0pt}{0.5\baselineskip}{0pt}

\titleformat{\paragraph}[runin]{\bfseries\small}{}{0pt}{}[~~~~]
\titlespacing*{\paragraph}{0pt}{0pt}{0pt}

\RequirePackage{titletoc}
\setcounter{tocdepth}{1}
\titlecontents{section}
	[0pt]
	{\vspace{4pt}}
	{\tocfont\thecontentslabel.~~}
	{\tocfont}
	{\titlerule*[4pt]{.}\tocfont\contentspage}
	[\vspace{1pt}]

\RequirePackage[usenames, svgnames]{xcolor}

\RequirePackage{booktabs}
\RequirePackage{array}
\renewcommand{\arraystretch}{1.25}
\newcolumntype{R}[1]{>{\raggedleft\arraybackslash}p{#1}}
\newcolumntype{L}[1]{>{\raggedright\arraybackslash}p{#1}}
\newcolumntype{C}[1]{>{\centering\arraybackslash}p{#1}}

\RequirePackage{caption}
\captionsetup{
	skip            = 6pt,
	font            = small,
	justification   = raggedright,
	singlelinecheck = false,
}

\RequirePackage{enumitem}
\setlist{itemsep=0pt, topsep=0pt}
\renewcommand{\labelenumi}{\arabic{enumi}.}
\renewcommand{\labelenumii}{\alph{enumii}.}
\renewcommand{\labelenumiii}{\roman{enumiii}.}
\renewcommand{\labelitemi}{\Large\raisebox{-1.5pt}{\textbullet}}
\renewcommand{\labelitemii}{\Large\raisebox{-1.5pt}{--}}
\renewcommand{\labelitemiii}{\Large\raisebox{-1.5pt}{-}}

\RequirePackage{hyperref}
\hypersetup{
	colorlinks = true,
	urlcolor   = black,
	linkcolor  = black,
	citecolor  = black,
}

\RequirePackage{listings}
\lstset{
	numbers     = left,
	numbersep   = 6pt,
	numberstyle = \tiny\it,
	frame       = single,
	basicstyle  = \tt\scriptsize,
	breaklines  = true,
	aboveskip   = 1.5\baselineskip,
	tabsize     = 3,
}

\newcommand{\reporttitle}[1]{\renewcommand{\reporttitle}{#1}}
\newcommand{\reportsubtitle}[1]{\renewcommand{\reportsubtitle}{#1}}
\newcommand{\reportauthors}[1]{\renewcommand{\reportauthors}{#1}}
\newcommand{\reportdate}[1]{\renewcommand{\reportdate}{#1}}
\newcommand{\leftheadercontent}[1]{\renewcommand{\leftheadercontent}{#1}}
\newcommand{\rightheadercontent}[1]{\renewcommand{\rightheadercontent}{#1}}
