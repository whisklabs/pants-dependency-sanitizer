
# Tool for cleaning Pants dependencies.

This tool works with a report from the followed command (run from the root folder of an inspected project):

    ./pants -q dep-usage.jvm --no-summary src/scala/:: > deps.json
     
Make this report before using this tool, it may take a few minutes!

## How to build

Binary (cargo required - [how to install rust](https://www.rust-lang.org/tools/install))

    cargo build --release
    
Docker (docker required)(may take a few minutes)

    docker build -t pants-cleaner .
    
    
## How to use

Run this tool from the root of your project. If report file from isn't in a 
project root, specified report file via `--report_file` e.g.

    pants-cleaner --report_file=/tmp/deps.json unused show 

For getting help

    pants-cleaner help    
    
For showing all unused dependencies

    pants-cleaner unused show 
    
For remove all unused dependencies

    pants-cleaner unused fix
    
For showing all undeclared but used transitively modules dependencies

    pants-cleaner undeclared show 
    
For adding all undeclared dependencies to corresponded BUILD files

    pants-cleaner undeclared fix
    
Use with docker 

    docker run -v ${PWD}/../atlas:/project/ pants-cleaner unused show 
  
Note that, we mount 'atlas' project into container as 'project' folder. This folder 
should contain 'deps.json' file as well.   
    

    
