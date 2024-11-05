# -*- mode: ruby -*-
# vi: set ft=ruby :
Vagrant.configure("2") do |config|
  config.vm.box = "openindiana/hipster"
  config.vm.box_version = "2024.11"
  config.vm.box_check_update = true
  config.vm.provider "virtualbox" do |vb|
    vb.memory = "6144"
  end
  config.vm.provision "shell", inline: <<-SHELL
    pkg install build-essential rustc
  SHELL
  config.vm.provision "shell", privileged: false, inline: <<-SHELL
    mkdir -p $HOME/.cargo
    cat << EOF > $HOME/.cargo/config.toml
[build]
target-dir = "$HOME/target"
EOF
    SHELL
end
